use super::*;

fn set(tracker: Tracker, entity: &str, field: &str, value: &str) -> Op {
    Op::Set {
        tracker,
        entity: entity.to_owned(),
        field: field.to_owned(),
        value: value.to_owned(),
    }
}

#[test]
fn parses_every_tracker_line() {
    let parsed = parse_delta(
        "<think>reasoning [Scene] Time: no</think>\n\
         [Scene] Time: вечер -> ночь\n\
         [Characters] Аква · Outfit: синее платье → банное полотенце · Location: баня\n\
         [Characters] - Даркнесс\n\
         [Bonds] Аква · Trust: 40 -> 55 (спас её)\n\
         [Inventory] + Казума: верёвка, факел\n\
         [Inventory] - Казума: верёвка\n\
         [NPC] Трактирщик · Attitude: насторожен -> дружелюбен\n\
         [Notebook] + T1: трактирщик — демон | hidden from: Аква, Казума\n\
         [Notebook] Clue T1: «у его тени рога»\n\
         [Notebook] Reveal T2\n",
    );
    assert!(parsed.skipped.is_empty(), "{:?}", parsed.skipped);
    assert_eq!(
        parsed.ops,
        vec![
            set(Tracker::Scene, "", "Time", "ночь"),
            set(Tracker::Characters, "Аква", "Outfit", "банное полотенце"),
            set(Tracker::Characters, "Аква", "Location", "баня"),
            Op::Remove {
                tracker: Tracker::Characters,
                entity: "Даркнесс".into()
            },
            set(Tracker::Bonds, "Аква", "Trust", "55 (спас её)"),
            Op::ItemAdd {
                owner: "Казума".into(),
                item: "верёвка".into()
            },
            Op::ItemAdd {
                owner: "Казума".into(),
                item: "факел".into()
            },
            Op::ItemRemove {
                owner: "Казума".into(),
                item: "верёвка".into()
            },
            set(Tracker::Npcs, "Трактирщик", "Attitude", "дружелюбен"),
            Op::Truth {
                id: "T1".into(),
                truth: "трактирщик — демон".into(),
                hidden_from: "Аква, Казума".into()
            },
            Op::Clue {
                id: "T1".into(),
                text: "у его тени рога".into(),
                found: None
            },
            Op::Reveal {
                id: "T2".into(),
                revealed: true
            },
        ]
    );
}

#[test]
fn section_headers_bullets_and_no_changes() {
    let parsed = parse_delta(
        "```\nScene:\n- [Characters] Aqua.Outfit: towel\n* Kazuma · Pose: sitting\n```",
    );
    assert_eq!(
        parsed.ops,
        vec![
            set(Tracker::Characters, "Aqua", "Outfit", "towel"),
            set(Tracker::Characters, "Kazuma", "Pose", "sitting"),
        ]
    );
    let none = parse_delta("No changes.");
    assert!(none.no_changes && none.ops.is_empty());
    let block = parse_delta(
        "noise\n<state_update>\n[Scene] Weather: дождь\n</state_update>\ntrailing [Scene] Time: x",
    );
    assert_eq!(block.ops, vec![set(Tracker::Scene, "", "Weather", "дождь")]);
    let removal = parse_delta(
        "[Scene] Weather: дождь -> (none)\n[Notebook] Revealed T3\n[Notebook] Truth-: T4",
    );
    assert_eq!(
        removal.ops,
        vec![
            set(Tracker::Scene, "", "Weather", ""),
            Op::Reveal {
                id: "T3".into(),
                revealed: true
            },
            Op::Reveal {
                id: "T4".into(),
                revealed: true
            },
        ]
    );
    let junk = parse_delta("Here are the changes:\n[Characters] nonsense");
    assert!(junk.ops.is_empty());
    assert_eq!(junk.skipped.len(), 2);
}

#[test]
fn apply_sets_removes_and_matches_names_loosely() {
    let mut state = HelperState::default();
    apply(
        &mut state,
        &[
            set(Tracker::Characters, "Аква", "Outfit", "платье"),
            set(Tracker::Characters, "**аква**", "outfit", "полотенце"),
            set(Tracker::Characters, "Аква", "Location", "баня"),
            set(Tracker::Characters, "Аква", "Location", ""),
            Op::ItemAdd {
                owner: "Казума".into(),
                item: "Старая верёвка".into(),
            },
            Op::ItemAdd {
                owner: "казума".into(),
                item: "старая веревка".into(),
            },
            Op::ItemRemove {
                owner: "Казума".into(),
                item: "верёвка".into(),
            },
        ],
        "m1",
    );
    assert_eq!(state.characters.len(), 1);
    assert_eq!(
        state.characters[0].fields,
        vec![Field {
            name: "Outfit".into(),
            value: "полотенце".into()
        }]
    );
    assert!(
        state.inventory.is_empty(),
        "the last item left drops the holder"
    );
    apply(
        &mut state,
        &[Op::Remove {
            tracker: Tracker::Characters,
            entity: "АКВА".into(),
        }],
        "m2",
    );
    assert!(state.characters.is_empty());
}

#[test]
fn notebook_truths_clues_reveal_and_cap() {
    let mut state = HelperState::default();
    apply(
        &mut state,
        &[
            Op::Truth {
                id: "T1".into(),
                truth: "a".into(),
                hidden_from: "Аква".into(),
            },
            Op::Clue {
                id: "T1".into(),
                text: "hint".into(),
                found: Some(true),
            },
            Op::Clue {
                id: "T9".into(),
                text: "orphan".into(),
                found: None,
            },
        ],
        "m1",
    );
    assert_eq!(
        state.notebook[0].clues,
        vec![Clue {
            text: "hint".into(),
            message_id: "m1".into(),
            found: Some(true)
        }]
    );
    apply(
        &mut state,
        &[Op::Reveal {
            id: "t1".into(),
            revealed: true,
        }],
        "m2",
    );
    assert!(state.notebook[0].revealed);
    assert_eq!(live_truths(&state), 0);
    let ops = (1..=5)
        .map(|n| Op::Truth {
            id: format!("T{}", n + 1),
            truth: "x".into(),
            hidden_from: String::new(),
        })
        .chain([Op::Truth {
            id: "T1".into(),
            truth: "rewritten".into(),
            hidden_from: String::new(),
        }])
        .collect::<Vec<_>>();
    let kept = limit_new_truths(ops, &state, MAX_LIVE_TRUTHS);
    assert_eq!(
        kept.len(),
        4,
        "three new live truths plus the rewrite: {kept:?}"
    );
}

#[test]
fn swipes_edits_and_deletes_roll_back_by_variant() {
    let delta = |id: &str, message: &str, variant: &str, ops: Vec<Op>, at: i64| StoredDelta {
        id: id.into(),
        message_id: message.into(),
        variant_key: variant.into(),
        source: DeltaSource::Helper,
        ops,
        created_at: at,
    };
    let rows = vec![
        delta(
            "base",
            "",
            "",
            vec![set(Tracker::Scene, "", "Place", "таверна")],
            0,
        ),
        delta(
            "a",
            "m1",
            "c:1",
            vec![set(Tracker::Characters, "Аква", "Outfit", "платье")],
            1,
        ),
        delta(
            "b",
            "m2",
            "c:2",
            vec![set(Tracker::Characters, "Аква", "Outfit", "полотенце")],
            2,
        ),
        delta(
            "b2",
            "m2",
            "c:3",
            vec![set(Tracker::Characters, "Аква", "Outfit", "доспех")],
            3,
        ),
        StoredDelta {
            source: DeltaSource::Manual,
            ..delta(
                "manual",
                "m2",
                "c:2",
                vec![set(Tracker::Scene, "", "Time", "ночь")],
                4,
            )
        },
    ];
    let path = |items: &[(&str, &str)]| {
        items
            .iter()
            .map(|(m, v)| ((*m).to_owned(), (*v).to_owned()))
            .collect::<Vec<_>>()
    };
    let outfit = |state: &HelperState| state.characters[0].fields[0].value.clone();

    let first = state_for_path(
        &rows,
        &path(&[("u1", "r:u1"), ("m1", "c:1"), ("u2", "r:u2"), ("m2", "c:2")]),
    );
    assert_eq!(outfit(&first), "полотенце");
    assert_eq!(first.scene.len(), 2);
    // Swipe to another variant: its own changes, the first variant's are gone.
    let swiped = state_for_path(&rows, &path(&[("m1", "c:1"), ("m2", "c:3")]));
    assert_eq!(outfit(&swiped), "доспех");
    assert_eq!(
        swiped.scene.len(),
        1,
        "the manual edit belonged to the other variant"
    );
    // Edit (a new revision) or delete: back to the state before that reply.
    let edited = state_for_path(&rows, &path(&[("m1", "c:1"), ("m2", "r:9")]));
    assert_eq!(outfit(&edited), "платье");
    let deleted = state_for_path(&rows, &path(&[("m1", "c:1")]));
    assert_eq!(deleted, edited);
}

#[test]
fn diff_round_trips_manual_edits() {
    let mut old = HelperState::default();
    apply(
        &mut old,
        &[
            set(Tracker::Scene, "", "Time", "ночь"),
            set(Tracker::Bonds, "Аква", "Trust", "40"),
            set(Tracker::Npcs, "Страж", "Role", "охрана"),
            Op::ItemAdd {
                owner: "Казума".into(),
                item: "меч".into(),
            },
            Op::Truth {
                id: "T1".into(),
                truth: "секрет".into(),
                hidden_from: String::new(),
            },
        ],
        "m",
    );
    let mut new = old.clone();
    new.scene[0].value = "утро".into();
    new.bonds[0].fields.push(Field {
        name: "Affection".into(),
        value: "70".into(),
    });
    new.npcs.clear();
    new.inventory[0].items = vec!["щит".into()];
    new.notebook[0].revealed = true;
    new.notebook.push(Truth {
        id: "T2".into(),
        truth: "новое".into(),
        ..Truth::default()
    });
    let ops = diff(&old, &new);
    let mut replayed = old.clone();
    apply(&mut replayed, &ops, "m");
    assert_eq!(replayed, new);
    assert!(diff(&new, &new).is_empty());
}

#[test]
fn renders_only_enabled_trackers() {
    let mut state = HelperState::default();
    apply(
        &mut state,
        &[
            set(Tracker::Scene, "", "Time", "ночь"),
            set(Tracker::Characters, "Аква", "Outfit", "полотенце"),
            Op::ItemAdd {
                owner: "Казума".into(),
                item: "меч".into(),
            },
            Op::Truth {
                id: "T1".into(),
                truth: "секрет".into(),
                hidden_from: "Аква".into(),
            },
            Op::Clue {
                id: "T1".into(),
                text: "намёк".into(),
                found: Some(false),
            },
        ],
        "m",
    );
    let all = render_state_block(&state, &Trackers::default(), "[Current state]").expect("block");
    assert_eq!(
        all,
        "[Current state]\nScene: Time: ночь\nCharacters:\n- Аква: Outfit: полотенце\nInventory:\n- Казума: меч\n\
         Hidden truths (author's notebook; secret, reveal only when the story earns it; drop subtle breadcrumbs):\n\
         - T1: секрет (hidden from: Аква) [clues so far: 0]"
    );
    let only_scene = Trackers {
        characters: false,
        bonds: false,
        inventory: false,
        npcs: false,
        notebook: false,
        ..Trackers::default()
    };
    assert_eq!(
        render_state_block(&state, &only_scene, "").as_deref(),
        Some("Scene: Time: ночь")
    );
    assert!(render_state_block(&HelperState::default(), &Trackers::default(), "x").is_none());
}

#[test]
fn clue_check_finds_phrases_and_inflected_words() {
    let reply = "Она заметила, что у его тени были рога, и быстро отвернулась.";
    assert!(clue_found("у его тени были рога", reply));
    assert!(
        clue_found("тень с рогами", reply),
        "stems match across endings"
    );
    assert!(!clue_found("кровь на полу таверны", reply));
    assert!(!clue_found("", reply));
}

#[test]
fn ops_serialize_with_tags() {
    let json = serde_json::to_string(&set(Tracker::Npcs, "A", "Role", "b")).expect("json");
    assert_eq!(
        json,
        r#"{"op":"set","tracker":"npcs","entity":"A","field":"Role","value":"b"}"#
    );
    let back: Op = serde_json::from_str(r#"{"op":"reveal","id":"T1"}"#).expect("default revealed");
    assert_eq!(
        back,
        Op::Reveal {
            id: "T1".into(),
            revealed: true
        }
    );
}

#[test]
fn prompt_fills_names_and_lists_trackers() {
    let system = helper_system_prompt("", "Казума", "Аква");
    assert!(system.contains("NSFW/NSFL allowed; all descriptions explicit."));
    assert!(system.contains("Russian"));
    assert!(system.contains("Казума's backstory"));
    let user = helper_user_message(&HelperInput {
        character_name: "Аква".into(),
        user_name: "Казума".into(),
        latest: vec![HelperLine {
            speaker: "Аква".into(),
            text: "Привет".into(),
        }],
        trackers: Trackers {
            notebook: false,
            ..Trackers::default()
        },
        ..HelperInput::default()
    });
    assert!(user.contains("Enabled trackers: Scene, Characters, Bonds, Inventory, NPC"));
    assert!(user.contains("<latest_exchange>\nАква: Привет\n</latest_exchange>"));
    assert!(!user.contains("Notebook:"));
}

struct FakeSource {
    chat: &'static str,
    alive: std::sync::atomic::AtomicBool,
}

impl HelperStateSource for FakeSource {
    fn trackers(&self, conversation_id: &str, _: &[String]) -> Option<Trackers> {
        (conversation_id == self.chat).then(Trackers::default)
    }

    fn deltas(&self, conversation_id: &str) -> Vec<StoredDelta> {
        if conversation_id != self.chat {
            return Vec::new();
        }
        vec![StoredDelta {
            id: self.chat.to_owned(),
            message_id: String::new(),
            variant_key: String::new(),
            source: DeltaSource::Manual,
            ops: vec![set(Tracker::Scene, "", "Место", self.chat)],
            created_at: 1,
        }]
    }

    fn alive(&self) -> bool {
        self.alive.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[test]
fn every_open_database_answers_for_its_own_chats() {
    let first = std::sync::Arc::new(FakeSource {
        chat: "fork-helper-source-a",
        alive: true.into(),
    });
    let second = std::sync::Arc::new(FakeSource {
        chat: "fork-helper-source-b",
        alive: true.into(),
    });
    set_state_source(first.clone());
    set_state_source(second.clone());
    let source = state_source().expect("sources");
    assert!(source.trackers("fork-helper-source-a", &[]).is_some());
    assert_eq!(source.deltas("fork-helper-source-b")[0].id, "fork-helper-source-b");
    assert!(source.trackers("fork-helper-source-c", &[]).is_none());
    first.alive.store(false, std::sync::atomic::Ordering::SeqCst);
    let source = state_source().expect("sources");
    assert!(source.trackers("fork-helper-source-a", &[]).is_none(), "a closed database drops out");
    assert!(source.trackers("fork-helper-source-b", &[]).is_some());
}
