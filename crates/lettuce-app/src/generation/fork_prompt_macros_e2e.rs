//! End to end: a character card with macros, assembled into a real prompt.

use std::sync::Arc;

use lettuce_characters::{
    Character, CharacterDefaults, CharacterMedia, CharacterPresentationV1, CharacterProfile,
    CharacterProvenance, CharacterRepository, CreateCharacterPlan,
};
use lettuce_context::fork_macros;
use lettuce_conversations::{
    ContextAssembler, ContextRequest, ConversationCreator, ConversationReader,
    ConversationRepository, GenerationInput, IdempotencyKey, MessageDraft, MessagePart,
    MessageRole, MessageVisibility, OperationToken, ParticipantRole, ProviderContextPart,
    SafetyContext, SendConversation,
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

fn seed_character(database: &Database, description: &str) -> CharacterId {
    let id = CharacterId::new();
    let character = Character::new(
        id,
        CharacterProfile {
            name: "Ada".into(),
            nickname: None,
            description: Some(description.into()),
            definition: None,
            design_description: None,
            scenario: None,
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
                    text: "Hello {{getvar::mood}}".into(),
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
async fn card_macros_run_in_their_slot_save_variables_and_log_dice() {
    let database = Database::open_in_memory().expect("database");
    crate::BuiltInPromptService::new(&database)
        .expect("prompt service")
        .bootstrap(TimestampMillis::new(1))
        .expect("bootstrap prompts");
    let store_db = Arc::new(Database::open_in_memory().expect("store"));
    fork_macros::set_var_store(Arc::new(DatabaseMacroVarStore(store_db.clone())));

    let character = seed_character(
        &database,
        "{{setvar::mood::calm}}{{incvar::turns}}{{// hidden}}Ada feels {{getvar::mood}}. Fate: {{roll::1d1+4}}. {{setglobalvar::fork-e2e-theme::Sola}}",
    );
    let (conversation_id, messages) = assemble_once(&database, character, "fork-macros-e2e").await;
    let text = joined(&messages);
    assert!(text.contains("Ada feels calm. Fate: 5."), "{text}");
    assert!(
        !text.contains("{{setvar") && !text.contains("hidden"),
        "{text}"
    );
    // History is not evaluated.
    assert!(text.contains("Hello {{getvar::mood}}"), "{text}");

    let vars = store_db.fork_chat_vars(&conversation_id).expect("vars");
    assert_eq!(vars["mood"], "calm");
    assert_eq!(vars["turns"], "1");
    assert_eq!(
        store_db.fork_global_vars().expect("globals")["fork-e2e-theme"],
        "Sola"
    );
    let rolls = store_db
        .fork_rolls_for(&conversation_id, &super::fingerprint(&messages))
        .expect("rolls")
        .expect("logged");
    assert_eq!(rolls.len(), 1);
    assert_eq!(rolls[0].total, 5);
}
