//! End to end: an AetherKit card assembled into a real prompt.

use std::sync::Arc;

use lettuce_characters::{
    Character, CharacterDefaults, CharacterMedia, CharacterPresentationV1, CharacterProfile,
    CharacterProvenance, CharacterRepository, CreateCharacterPlan,
};
use lettuce_context::fork_macros;
use lettuce_conversations::{
    ContextAssembler, ContextRequest, ConversationCreator, ConversationReader, ConversationRepository,
    GenerationInput, IdempotencyKey, MessageDraft, MessagePart, MessageRole, MessageVisibility,
    OperationToken, ParticipantRole, ProviderContextPart, SafetyContext, SendConversation,
};
use lettuce_database::{Database, DatabaseMacroVarStore};
use lettuce_types::{CharacterId, ContentHash, TimestampMillis};

use crate::launch::{
    ConversationLaunchPlanner, DirectConversationLaunchRequest, DirectUserParticipant,
    LaunchSelection,
};

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::new(value).expect("key")
}

fn seed_character(
    database: &Database,
    description: &str,
    definition: Option<&str>,
    scenario: Option<&str>,
) -> CharacterId {
    let id = CharacterId::new();
    let character = Character::new(
        id,
        CharacterProfile {
            name: "Mara".into(),
            nickname: None,
            description: Some(description.into()),
            definition: definition.map(str::to_owned),
            design_description: None,
            scenario: scenario.map(str::to_owned),
        },
        CharacterProvenance::default(),
        CharacterDefaults::default(),
        CharacterPresentationV1::default(),
        None,
        CharacterMedia::default(),
        TimestampMillis::new(1),
    )
    .expect("character");
    CharacterRepository::create(
        database,
        CreateCharacterPlan {
            character,
            scenes: Vec::new(),
            variants: Vec::new(),
            starters: Vec::new(),
        },
    )
    .expect("create character");
    id
}

async fn assemble_once(
    database: &Database,
    character_id: CharacterId,
    operation: &str,
) -> (String, Vec<lettuce_conversations::ProviderNeutralMessage>) {
    let prepared = ConversationLaunchPlanner::new(database)
        .prepare_direct(&DirectConversationLaunchRequest {
            format_version: 1,
            title: "Session".into(),
            user: DirectUserParticipant {
                display_name: "Traveller".into(),
                authored_description: None,
            },
            character_id,
            scene: LaunchSelection::Inherit,
            starter: LaunchSelection::Inherit,
            persona: LaunchSelection::Inherit,
            operation_key: key(operation),
        })
        .expect("prepare");
    let launched = ConversationCreator::create(database, prepared, TimestampMillis::new(10))
        .expect("create chat");
    let conversation = &launched.value.conversation;
    let user_id = conversation
        .participants
        .iter()
        .find(|participant| participant.role == ParticipantRole::User)
        .expect("user")
        .id;
    let sent = ConversationRepository::begin_send(
        database,
        &SendConversation {
            conversation_id: conversation.id,
            branch_id: conversation.active_branch_id,
            expected_revision: conversation.revision,
            operation: OperationToken {
                key: key(&format!("{operation}-send")),
                request_digest: ContentHash::parse("ab".repeat(32)).expect("digest"),
            },
            message: MessageDraft {
                role: MessageRole::User,
                author_participant_id: Some(user_id),
                parts: vec![MessagePart::Text {
                    text: "Hello.".into(),
                }],
                visibility: MessageVisibility::Visible,
                pinned: false,
                scene_edited: false,
            },
            swap_roles: false,
        },
        TimestampMillis::new(11),
    )
    .expect("send");
    let GenerationInput::UserMessage { message_id } = sent.value.turn.input else {
        panic!("user message expected");
    };
    let aggregate = ConversationReader::get(database, conversation.id).expect("conversation");
    let branch_id = aggregate.conversation.active_branch_id;
    let request = ContextRequest {
        conversation_id: conversation.id,
        branch_id,
        branch_path: vec![branch_id],
        source_message_id: message_id,
        operation: lettuce_conversations::GenerationOperation::Send,
        swap_roles: false,
        guidance: None,
        window: lettuce_conversations::ContextWindowPolicy::default(),
        selected_speaker: None,
        capabilities: lettuce_models::ModelCapabilities::default(),
        safety: SafetyContext::Standard,
        prompt_runtime: lettuce_conversations::PromptRuntimeFacts::default(),
        prompt_values: lettuce_conversations::PromptRuntimeValues::default(),
        reference_time: TimestampMillis::new(0),
        usage_snapshot: None,
        memory: None,
        timeline: ConversationReader::timeline_page(
            database,
            conversation.id,
            branch_id,
            &lettuce_types::PageRequest::default(),
        )
        .expect("timeline")
        .items,
    };
    let context = crate::ConversationContextAssembler::new(database)
        .assemble(request)
        .await
        .expect("assemble");
    (conversation.id.to_string(), context.messages)
}

fn joined(messages: &[lettuce_conversations::ProviderNeutralMessage]) -> String {
    messages
        .iter()
        .flat_map(|message| &message.parts)
        .filter_map(|part| match part {
            ProviderContextPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn aetherkit_card_fields_fill_plugin_vars_and_strip_shorthand() {
    let database = Database::open_in_memory().expect("database");
    crate::BuiltInPromptService::new(&database)
        .expect("prompt service")
        .bootstrap(TimestampMillis::new(1))
        .expect("bootstrap prompts");
    let store_db = Arc::new(Database::open_in_memory().expect("store"));
    fork_macros::set_var_store(Arc::new(DatabaseMacroVarStore(store_db.clone())));

    // Lettuce's built-in prompt injects the card as {{char.desc}} (definition
    // if set, else description). Put the whole sample there, with getvars after
    // the shorthand so they run in the same field, after the setvars.
    let character = seed_character(
        &database,
        r#"Mara holds a damaged receiver.

[[flag1: signal_audible]]

[[hook1]]
condition: signal_audible = TRUE
call: missing_signal_listen
[[/hook1]]

[[spec: arc]]
name: The Missing Signal
objective: identify the source of the signal
focus: {{user}}
[[/spec: arc]]

[[state]]
<details>
  <summary>🧩 SIGNAL STATE</summary>
  <li><b>Damage:</b> 2</li>
</details>
[[/state]]

[[dice1]]
name: sanityRoll
roll: {{roll::1d1}}
[[/dice1]]

<fn:missing_signal_listen>
  Describe the signal through sounds {{user}} can hear.
  ⇤
</fn:missing_signal_listen>

FLAG={{getvar::flag1}}
HOOK={{getvar::hookCall1}}
ARC={{getvar::pluginArcSpec}}
STATE={{getvar::pluginStateTemplate}}
DICE={{getvar::pluginDice1}}
"#,
        None,
        None,
    );
    let (conversation_id, messages) = assemble_once(&database, character, "fork-aether-e2e").await;
    let text = joined(&messages);
    assert!(text.contains("Mara holds a damaged receiver."), "{text}");
    assert!(
        text.contains("<fn:missing_signal_listen>"),
        "function body stays: {text}"
    );
    assert!(!text.contains("[[flag1:"), "{text}");
    assert!(!text.contains("[[hook1]]"), "{text}");
    assert!(!text.contains("[[spec:"), "{text}");
    assert!(!text.contains("[[state]]"), "{text}");
    assert!(!text.contains("[[dice1]]"), "{text}");
    assert!(text.contains("FLAG=signal_audible"), "{text}");
    assert!(
        text.contains("HOOK=condition: signal_audible = TRUE\n➤ <fn:missing_signal_listen>"),
        "{text}"
    );
    assert!(text.contains("ARC=name: The Missing Signal"), "{text}");
    assert!(
        text.contains("focus: Traveller") || text.contains("focus: {{user}}"),
        "{text}"
    );
    assert!(text.contains("🧩 SIGNAL STATE"), "{text}");
    assert!(
        text.contains("STATE=") && text.contains("<details>"),
        "{text}"
    );
    assert!(text.contains("DICE=name: sanityRoll"), "{text}");
    assert!(text.contains("roll: 1"), "{text}");

    let vars = store_db.fork_chat_vars(&conversation_id).expect("vars");
    assert_eq!(vars["flag1"], "signal_audible");
    assert!(
        vars["hookCall1"].contains("➤ <fn:missing_signal_listen>"),
        "{}",
        vars["hookCall1"]
    );
    assert!(vars["pluginArcSpec"].contains("The Missing Signal"));
    assert!(vars["pluginStateTemplate"].contains("SIGNAL STATE"));
    assert!(vars["pluginDice1"].contains("roll: 1"));
}
