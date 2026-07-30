    use std::collections::BTreeMap;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use atto_ui::ComponentValue;
    use atto_ui::composable::{
        ComponentContext, EventHandling, MouseCoordinateSpace, ScrollbarHost, TabMode,
    };
    use atto_ui::theme::Theme;
    use atto_ui::wm::WindowId;
    use atto_ui_chat::{
        ApprovalAction, ApprovalDecision, ApprovalLevel, ChatBlock, ChatBlockId, ChatError,
        ChatErrorKind, ChatInputMode, ChatInputResponse, ChatMessage, ChatMessageStore, ChatRole,
        ChatSlashCommandAction, ChatTurnStatus, CompactBlock, CompactStatus, EditAndResubmitEvent,
        MessageAction, MessageActionKind, PlanBlock, PlanDecision, PlanDecisionEvent, PlanItem,
        StopReason, TokenUsage, ToolInput, ToolOutput, ToolResultBlock, ToolStatus, ToolUseBlock,
    };
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;

    use crate::compact::CompactPolicy;
    use crate::config::{AgentConfig, AgentProvider, PlanMode};
    use crate::deepseek::{
        ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionDelta,
        ChatCompletionSseEvent, ChatFunctionCallDelta, ChatMessageRole, ChatToolCallDelta,
        ChatToolKind, ToolChoice, ToolChoiceMode, chat_error_from_http_status,
        chat_error_from_json_error, chat_error_from_network_failure,
        chat_error_from_stream_disconnect, parse_chat_completion_sse,
        parse_chat_completion_sse_data,
    };
    use crate::skill::{LoadedSkillSet, SkillMode, SkillRegistry, SkillSearchPath};
    use crate::stream_ui::DeepSeekUiStream;
    use crate::tool::{
        ToolContext, ToolExecutor, ToolOutputKind, ToolPermission, ToolPermissionPolicy,
        ToolRegistry, ToolResult, ToolSpec,
    };

    use super::{
        ACCEPTED_PLAN_EXECUTION_INSTRUCTION, APP_TITLE, AgentApp, AgentTurnLauncher,
        AgentTurnLimits, AppAction, MISSING_DEEPSEEK_API_KEY_NOTICE, MockTurnRegistry,
        PLAN_MODE_MUTATING_TOOL_BLOCKED_RESULT, PlanDecisionRuntime, STATUS_READY,
        STATUS_STREAMING, SlashRuntime, StatusSegmentBindings, ToolRuntime, TranscriptPersistence,
        TranscriptStatusState, TurnBudgetTracker, append_startup_notices, apply_app_action,
        build_chat_panel, deepseek_plan_request_from_transcript, deepseek_request_from_transcript,
        deepseek_request_from_transcript_with_skills, error_summary_status,
        execute_tool_use_to_result_block, format_token_estimate_status, handle_edit_and_resubmit,
        handle_message_action, handle_plan_decision, handle_tool_approval, status_segments,
        submit_input_response, submit_slash_command_text, sync_transcript_status,
    };

    fn message_text(message: &ChatMessage) -> &str {
        match &message.blocks[0] {
            ChatBlock::Text(block) => &block.markdown,
            other => panic!("expected text block, got {other:?}"),
        }
    }

    fn failed_turn_error(message: &ChatMessage) -> &ChatError {
        let ChatTurnStatus::Failed(error) = &message.status else {
            panic!("expected failed turn, got {:?}", message.status);
        };
        error
    }

    fn plan_decision(store: &ChatMessageStore, block_id: ChatBlockId) -> PlanDecision {
        store
            .with_block(block_id, |block| match block {
                ChatBlock::Plan(plan) => plan.decision,
                other => panic!("expected plan block, got {other:?}"),
            })
            .expect("plan block should exist")
    }

    fn new_test_stream() -> DeepSeekUiStream {
        let store = ChatMessageStore::new();
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let text_block_id = assistant.blocks[0].id();
        store.push(assistant);
        DeepSeekUiStream::new(
            store.branch_token(),
            assistant_id,
            text_block_id,
            "deepseek-chat",
        )
    }

    fn tool_call_delta(
        index: u32,
        id: Option<&str>,
        name: Option<&str>,
        arguments: Option<&str>,
    ) -> ChatToolCallDelta {
        ChatToolCallDelta {
            index,
            id: id.map(str::to_string),
            kind: id.map(|_| ChatToolKind::Function),
            function: Some(ChatFunctionCallDelta {
                name: name.map(str::to_string),
                arguments: arguments.map(str::to_string),
            }),
        }
    }

    fn single_failed_error(actions: Vec<AppAction>) -> ChatError {
        match actions.as_slice() {
            [AppAction::TurnFailed { error, .. }] => error.clone(),
            other => panic!("expected one failed action, got {other:?}"),
        }
    }

    struct TestSkillState {
        registry: SkillRegistry,
        loaded: LoadedSkillSet,
        count_state: atto_ui::reactive::Property<String>,
    }

    impl TestSkillState {
        fn new(registry: SkillRegistry) -> Self {
            let loaded = LoadedSkillSet::default();
            let count_state = atto_ui::reactive::Property::new(loaded.status());
            Self {
                registry,
                loaded,
                count_state,
            }
        }
    }

    impl Default for TestSkillState {
        fn default() -> Self {
            Self::new(SkillRegistry::default())
        }
    }

    fn test_slash_runtime(
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        plan_mode_state: &atto_ui::reactive::Property<String>,
        turn_budgets: &TurnBudgetTracker,
    ) -> SlashRuntime {
        let skills = TestSkillState::default();
        test_slash_runtime_with_skills(
            input_handle,
            mock_turns,
            status_state,
            plan_mode_state,
            &skills,
            turn_budgets,
        )
    }

    fn test_slash_runtime_with_skills(
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        plan_mode_state: &atto_ui::reactive::Property<String>,
        skills: &TestSkillState,
        turn_budgets: &TurnBudgetTracker,
    ) -> SlashRuntime {
        SlashRuntime {
            input_handle: input_handle.clone(),
            mock_turns: mock_turns.clone(),
            status_state: status_state.clone(),
            plan_mode_state: plan_mode_state.clone(),
            skill_registry: skills.registry.clone(),
            loaded_skills: skills.loaded.clone(),
            skill_count_state: skills.count_state.clone(),
            transcript_status: TranscriptStatusState::new(),
            turn_budgets: turn_budgets.clone(),
        }
    }

    fn context(theme: &Theme) -> ComponentContext<'_> {
        ComponentContext {
            theme,
            window_id: WindowId::default(),
            is_focused: true,
            scrollbar_host: ScrollbarHost::Component,
            tab_mode: TabMode::Cycle,
            mouse_coordinate_space: MouseCoordinateSpace::Absolute,
            drag: None,
        }
    }

    fn test_tool_registry() -> ToolRegistry {
        crate::tool::builtin_tool_registry().expect("built-in tool registry must be valid")
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "atto-agent-app-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_test_skill(workspace: &Path, dir_name: &str, name: &str, description: &str) {
        write_test_skill_with_mode(
            workspace,
            dir_name,
            name,
            description,
            SkillMode::Manual,
            &[],
        );
    }

    fn write_test_skill_with_mode(
        workspace: &Path,
        dir_name: &str,
        name: &str,
        description: &str,
        mode: SkillMode,
        triggers: &[&str],
    ) {
        write_test_skill_with_mode_and_tools(
            workspace,
            dir_name,
            name,
            description,
            mode,
            triggers,
            &[],
        );
    }

    fn write_test_skill_with_mode_and_tools(
        workspace: &Path,
        dir_name: &str,
        name: &str,
        description: &str,
        mode: SkillMode,
        triggers: &[&str],
        tools: &[&str],
    ) {
        let dir = workspace.join(".atto/skills").join(dir_name);
        fs::create_dir_all(&dir).expect("test skill directory should be created");
        let triggers = triggers
            .iter()
            .map(|trigger| format!("\"{trigger}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let tools = tools
            .iter()
            .map(|tool| format!("\"{tool}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            dir.join("SKILL.md"),
            format!(
                r#"---
name: {name}
description: {description}
triggers: [{triggers}]
tools: [{tools}]
mode: {mode}
---
Use this skill for {name} tasks.
"#
            ),
        )
        .expect("test skill file should be written");
    }

    fn test_skill_registry(skills: &[(&str, &str, &str)]) -> (PathBuf, SkillRegistry) {
        let workspace = unique_temp_dir("skills");
        for (dir_name, name, description) in skills {
            write_test_skill(&workspace, dir_name, name, description);
        }
        let registry =
            SkillRegistry::discover_from_paths(&[SkillSearchPath::workspace(&workspace)]);
        (workspace, registry)
    }

    fn test_tool_permissions() -> Arc<Mutex<ToolPermissionPolicy>> {
        Arc::new(Mutex::new(ToolPermissionPolicy::default()))
    }

    fn test_tool_runtime(
        config: AgentConfig,
        action_sender: std::sync::mpsc::Sender<AppAction>,
        registry: ToolRegistry,
        permissions: Arc<Mutex<ToolPermissionPolicy>>,
    ) -> ToolRuntime {
        test_tool_runtime_with_limits(
            config,
            action_sender,
            registry,
            permissions,
            AgentTurnLimits::default(),
        )
    }

    fn test_tool_runtime_with_limits(
        config: AgentConfig,
        action_sender: std::sync::mpsc::Sender<AppAction>,
        registry: ToolRegistry,
        permissions: Arc<Mutex<ToolPermissionPolicy>>,
        limits: AgentTurnLimits,
    ) -> ToolRuntime {
        test_tool_runtime_with_limits_and_budgets(
            config,
            action_sender,
            registry,
            permissions,
            limits,
            TurnBudgetTracker::default(),
        )
    }

    fn test_tool_runtime_with_limits_and_budgets(
        config: AgentConfig,
        action_sender: std::sync::mpsc::Sender<AppAction>,
        registry: ToolRegistry,
        permissions: Arc<Mutex<ToolPermissionPolicy>>,
        limits: AgentTurnLimits,
        turn_budgets: TurnBudgetTracker,
    ) -> ToolRuntime {
        ToolRuntime {
            config,
            action_sender,
            registry,
            permissions,
            turn_budgets,
            limits,
            input_handle: atto_ui_chat::ChatInputHandle::new(),
            mock_turns: MockTurnRegistry::new(),
            status_state: atto_ui::reactive::Property::new(STATUS_READY.to_string()),
            skill_registry: SkillRegistry::default(),
            loaded_skills: LoadedSkillSet::default(),
            transcript_status: TranscriptStatusState::new(),
        }
    }

    struct LiveToolRuntimeParts<'a> {
        config: AgentConfig,
        action_sender: std::sync::mpsc::Sender<AppAction>,
        registry: ToolRegistry,
        turn_budgets: TurnBudgetTracker,
        limits: AgentTurnLimits,
        input_handle: &'a atto_ui_chat::ChatInputHandle,
        mock_turns: &'a MockTurnRegistry,
        status_state: &'a atto_ui::reactive::Property<String>,
        transcript_status: &'a TranscriptStatusState,
    }

    fn live_tool_runtime(parts: LiveToolRuntimeParts<'_>) -> ToolRuntime {
        ToolRuntime {
            config: parts.config,
            action_sender: parts.action_sender,
            registry: parts.registry,
            permissions: test_tool_permissions(),
            turn_budgets: parts.turn_budgets,
            limits: parts.limits,
            input_handle: parts.input_handle.clone(),
            mock_turns: parts.mock_turns.clone(),
            status_state: parts.status_state.clone(),
            skill_registry: SkillRegistry::default(),
            loaded_skills: LoadedSkillSet::default(),
            transcript_status: parts.transcript_status.clone(),
        }
    }

    fn apply_live_actions_until_idle(
        receiver: &std::sync::mpsc::Receiver<AppAction>,
        store: &ChatMessageStore,
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        transcript_status: &TranscriptStatusState,
        tool_runtime: &ToolRuntime,
    ) {
        for _ in 0..16 {
            let action = receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("live DeepSeek tool loop should emit an app action");
            apply_app_action(
                store,
                input_handle,
                mock_turns,
                status_state,
                transcript_status,
                tool_runtime,
                action,
            );
            if !input_handle.streaming_binding().get() {
                return;
            }
        }
        panic!("live DeepSeek tool loop did not become idle");
    }

    struct LivePromptResult {
        messages: Vec<ChatMessage>,
        streaming: bool,
        status: String,
        error_summary: String,
    }

    struct LiveHttpErrorResult {
        error: ChatError,
        error_summary: String,
        request: String,
    }

    fn run_live_prompt_to_idle(config: AgentConfig, prompt: &str) -> LivePromptResult {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let registry = test_tool_registry();
        let limits = AgentTurnLimits::default();
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender.clone(),
            tool_registry: registry.clone(),
            turn_budgets: turn_budgets.clone(),
            limits,
            compact_policy: CompactPolicy::default(),
        };
        let transcript_status = TranscriptStatusState::new();
        let tool_runtime = live_tool_runtime(LiveToolRuntimeParts {
            config,
            action_sender: sender,
            registry,
            turn_budgets: turn_budgets.clone(),
            limits,
            input_handle: &input_handle,
            mock_turns: &mock_turns,
            status_state: &status_state,
            transcript_status: &transcript_status,
        });

        submit_input_response(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            &turn_launcher,
            ChatInputResponse::Text(prompt.to_string()),
        );
        apply_live_actions_until_idle(
            &receiver,
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
        );

        LivePromptResult {
            messages: store.messages(),
            streaming: input_handle.streaming_binding().get(),
            status: status_state.get(),
            error_summary: transcript_status.error_summary_state.get(),
        }
    }

    fn live_http_error(
        status: u16,
        reason: &'static str,
        body: &'static str,
    ) -> LiveHttpErrorResult {
        let server = TestSseServer::spawn_response(status, reason, "application/json", body);
        let mut config = AgentConfig::defaults(".");
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;

        let result = run_live_prompt_to_idle(config, "live http error prompt");
        let request = server.join();
        let error = failed_turn_error(&result.messages[1]).clone();

        assert_eq!(result.status, STATUS_READY);
        assert!(!result.streaming);
        LiveHttpErrorResult {
            error,
            error_summary: result.error_summary,
            request,
        }
    }

    fn test_plan_decision_runtime(
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        turn_budgets: &TurnBudgetTracker,
    ) -> (PlanDecisionRuntime, std::sync::mpsc::Receiver<AppAction>) {
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        (
            PlanDecisionRuntime {
                input_handle: input_handle.clone(),
                mock_turns: mock_turns.clone(),
                status_state: status_state.clone(),
                skill_registry: SkillRegistry::default(),
                loaded_skills: LoadedSkillSet::default(),
                transcript_status: TranscriptStatusState::new(),
                turn_launcher: AgentTurnLauncher {
                    config: AgentConfig::defaults("."),
                    action_sender: sender,
                    tool_registry: test_tool_registry(),
                    turn_budgets: turn_budgets.clone(),
                    limits: AgentTurnLimits::default(),
                    compact_policy: CompactPolicy::default(),
                },
            },
            receiver,
        )
    }

    fn test_turn_launcher(
        action_sender: std::sync::mpsc::Sender<AppAction>,
        turn_budgets: &TurnBudgetTracker,
        compact_policy: CompactPolicy,
    ) -> AgentTurnLauncher {
        AgentTurnLauncher {
            config: AgentConfig::defaults("."),
            action_sender,
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy,
        }
    }

    fn apply_test_app_action(
        store: &ChatMessageStore,
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        action: AppAction,
    ) -> bool {
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime =
            test_tool_runtime(AgentConfig::defaults("."), sender, registry, permissions);
        let transcript_status = TranscriptStatusState::new();
        apply_app_action(
            store,
            input_handle,
            mock_turns,
            status_state,
            &transcript_status,
            &tool_runtime,
            action,
        )
    }

    fn run_command_tool_call(call_id: &str) -> ToolUseBlock {
        ToolUseBlock {
            id: atto_ui_chat::ChatBlockId::new(0),
            call_id: call_id.to_string(),
            name: "run_command".to_string(),
            input: ToolInput::Json(ComponentValue::Map(BTreeMap::from([(
                "argv".to_string(),
                ComponentValue::List(vec![ComponentValue::String("cargo".to_string())]),
            )]))),
            status: ToolStatus::Pending,
            approval: None,
            collapsed: false,
        }
    }

    fn read_file_tool_call(call_id: &str, path: &str) -> ToolUseBlock {
        ToolUseBlock {
            id: atto_ui_chat::ChatBlockId::new(0),
            call_id: call_id.to_string(),
            name: "read_file".to_string(),
            input: ToolInput::Json(ComponentValue::Map(BTreeMap::from([(
                "path".to_string(),
                ComponentValue::String(path.to_string()),
            )]))),
            status: ToolStatus::Pending,
            approval: None,
            collapsed: false,
        }
    }

    fn unknown_tool_call(call_id: &str) -> ToolUseBlock {
        ToolUseBlock {
            id: atto_ui_chat::ChatBlockId::new(0),
            call_id: call_id.to_string(),
            name: "missing_tool".to_string(),
            input: ToolInput::Json(ComponentValue::Map(BTreeMap::new())),
            status: ToolStatus::Pending,
            approval: None,
            collapsed: false,
        }
    }

    #[derive(Clone, Copy)]
    struct SlowTool {
        delay: std::time::Duration,
    }

    impl ToolExecutor for SlowTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec::new(
                "slow_tool",
                "Sleep long enough to exercise app-level tool timeout handling.",
                serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                ToolPermission::AlwaysAllow,
                ToolOutputKind::Markdown,
            )
            .expect("slow test tool spec should be valid")
        }

        fn execute(
            &self,
            _ctx: ToolContext,
            _args: serde_json::Value,
        ) -> anyhow::Result<ToolResult> {
            std::thread::sleep(self.delay);
            Ok(ToolResult::success(
                "slow tool finished",
                ToolOutputKind::Markdown,
            ))
        }
    }

    struct TestSseServer {
        address: String,
        handle: thread::JoinHandle<String>,
    }

    struct TestSseSequenceServer {
        address: String,
        handle: thread::JoinHandle<Vec<String>>,
    }

    struct TestAbortableSseServer {
        address: String,
        first_event_sent: mpsc::Receiver<()>,
        handle: thread::JoinHandle<(String, bool)>,
    }

    impl TestSseServer {
        fn spawn(body: impl Into<String>) -> Self {
            Self::spawn_response(200, "OK", "text/event-stream", body)
        }
        fn spawn_response(
            status: u16,
            reason: &'static str,
            content_type: &'static str,
            body: impl Into<String>,
        ) -> Self {
            let body = body.into();
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock SSE server");
            listener
                .set_nonblocking(true)
                .expect("configure mock SSE listener");
            let address = listener
                .local_addr()
                .expect("mock SSE server address")
                .to_string();
            let handle = thread::spawn(move || {
                let (mut stream, _) = accept_with_timeout(&listener, Duration::from_secs(5));
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("configure mock SSE read timeout");
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .expect("configure mock SSE write timeout");
                let request = read_http_request(&mut stream);
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("write mock SSE response");
                request
            });
            Self { address, handle }
        }

        fn base_url(&self) -> String {
            format!("http://{}/v1", self.address)
        }

        fn join(self) -> String {
            self.handle.join().expect("mock SSE server should join")
        }
    }

    impl TestAbortableSseServer {
        fn spawn(first_event: impl Into<String>) -> Self {
            let first_event = first_event.into();
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock SSE server");
            listener
                .set_nonblocking(true)
                .expect("configure mock SSE listener");
            let address = listener
                .local_addr()
                .expect("mock SSE server address")
                .to_string();
            let (sent_tx, first_event_sent) = mpsc::channel();
            let handle = thread::spawn(move || {
                let (mut stream, _) = accept_with_timeout(&listener, Duration::from_secs(5));
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("configure mock SSE read timeout");
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .expect("configure mock SSE write timeout");
                let request = read_http_request(&mut stream);
                let response_headers = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                stream
                    .write_all(response_headers.as_bytes())
                    .expect("write mock SSE response headers");
                stream
                    .write_all(first_event.as_bytes())
                    .expect("write first mock SSE event");
                stream.flush().expect("flush first mock SSE event");
                sent_tx.send(()).ok();
                let closed = wait_for_client_close(&mut stream, Duration::from_secs(3));
                (request, closed)
            });
            Self {
                address,
                first_event_sent,
                handle,
            }
        }

        fn base_url(&self) -> String {
            format!("http://{}/v1", self.address)
        }

        fn wait_for_first_event(&self) {
            self.first_event_sent
                .recv_timeout(Duration::from_secs(2))
                .expect("mock SSE server should send the first event");
        }

        fn join(self) -> (String, bool) {
            self.handle.join().expect("mock SSE server should join")
        }
    }

    impl TestSseSequenceServer {
        fn spawn(bodies: Vec<String>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock SSE server");
            listener
                .set_nonblocking(true)
                .expect("configure mock SSE listener");
            let address = listener
                .local_addr()
                .expect("mock SSE server address")
                .to_string();
            let handle = thread::spawn(move || {
                let mut requests = Vec::new();
                for body in bodies {
                    let (mut stream, _) = accept_with_timeout(&listener, Duration::from_secs(5));
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .expect("configure mock SSE read timeout");
                    stream
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .expect("configure mock SSE write timeout");
                    requests.push(read_http_request(&mut stream));
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    stream
                        .write_all(response.as_bytes())
                        .expect("write mock SSE response");
                }
                requests
            });
            Self { address, handle }
        }

        fn base_url(&self) -> String {
            format!("http://{}/v1", self.address)
        }

        fn join(self) -> Vec<String> {
            self.handle.join().expect("mock SSE server should join")
        }
    }

    fn sse_tool_call_body(call_id: &str, name: &str, arguments: serde_json::Value) -> String {
        let chunk = serde_json::json!({
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": arguments.to_string()
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        format!("data: {chunk}\n\ndata: [DONE]\n\n")
    }

    fn sse_final_text_body(text: &str) -> String {
        let chunk = serde_json::json!({
            "model": "mock-deepseek",
            "choices": [{
                "index": 0,
                "delta": { "content": text },
                "finish_reason": "stop"
            }]
        });
        format!("data: {chunk}\n\ndata: [DONE]\n\n")
    }

    fn accept_with_timeout(
        listener: &TcpListener,
        timeout: Duration,
    ) -> (TcpStream, std::net::SocketAddr) {
        let start = Instant::now();
        loop {
            match listener.accept() {
                Ok((stream, address)) => {
                    stream
                        .set_nonblocking(false)
                        .expect("configure mock SSE stream blocking");
                    return (stream, address);
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && start.elapsed() < timeout =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept mock SSE request: {error}"),
            }
        }
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let read = stream.read(&mut buffer).expect("read mock HTTP request");
            if read == 0 {
                break;
            }

            bytes.extend_from_slice(&buffer[..read]);
            if http_request_body_complete(&bytes) {
                break;
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn wait_for_client_close(stream: &mut TcpStream, timeout: Duration) -> bool {
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("configure mock SSE close probe timeout");
        let start = Instant::now();
        let mut buffer = [0_u8; 1];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    if start.elapsed() >= timeout {
                        return false;
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionAborted
                            | std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::BrokenPipe
                    ) =>
                {
                    return true;
                }
                Err(error) => panic!("probe mock SSE client close: {error}"),
            }
        }
    }

    fn http_request_body_complete(bytes: &[u8]) -> bool {
        let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            return false;
        };
        let headers = String::from_utf8_lossy(&bytes[..header_end]);
        let content_length = headers.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        });
        let body_start = header_end + 4;
        bytes.len() >= body_start + content_length.unwrap_or(0)
    }

    fn http_request_json(request: &str) -> serde_json::Value {
        let (_, body) = request
            .split_once("\r\n\r\n")
            .expect("mock HTTP request should contain a header/body separator");
        serde_json::from_str(body).expect("mock HTTP request body should be JSON")
    }

    fn slow_tool_call(call_id: &str) -> ToolUseBlock {
        ToolUseBlock {
            id: atto_ui_chat::ChatBlockId::new(0),
            call_id: call_id.to_string(),
            name: "slow_tool".to_string(),
            input: ToolInput::Json(ComponentValue::Map(BTreeMap::new())),
            status: ToolStatus::Pending,
            approval: None,
            collapsed: false,
        }
    }

    fn test_workspace(name: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "atto-agent-app-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create fixture workspace");
        path
    }

    fn append_tool_call_with_runtime(
        store: &ChatMessageStore,
        input_handle: &atto_ui_chat::ChatInputHandle,
        mock_turns: &MockTurnRegistry,
        status_state: &atto_ui::reactive::Property<String>,
        registry: &ToolRegistry,
        permissions: &Arc<Mutex<ToolPermissionPolicy>>,
        tool_call: ToolUseBlock,
    ) -> atto_ui_chat::ChatBlockId {
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            AgentConfig::defaults("."),
            sender,
            registry.clone(),
            permissions.clone(),
        );
        let expected_call_id = tool_call.call_id.clone();
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        store.push(assistant);
        let branch = store.branch_token();
        let transcript_status = TranscriptStatusState::new();

        assert!(apply_app_action(
            store,
            input_handle,
            mock_turns,
            status_state,
            &transcript_status,
            &tool_runtime,
            AppAction::ToolCallsReady {
                branch,
                message_id: assistant_id,
                tool_calls: vec![tool_call],
                mutating_tools_allowed: true,
                continue_after_tools: false,
            },
        ));

        store
            .messages()
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) if tool.call_id == expected_call_id => Some(tool.id),
                _ => None,
            })
            .expect("tool use block should be appended")
    }

    fn tool_use_for_block(
        store: &ChatMessageStore,
        block_id: atto_ui_chat::ChatBlockId,
    ) -> ToolUseBlock {
        store
            .with_block(block_id, |block| match block {
                ChatBlock::ToolUse(tool) => Some(tool.clone()),
                other => panic!("expected tool use block, got {other:?}"),
            })
            .flatten()
            .expect("tool use block should exist")
    }

    fn tool_result_for_call(store: &ChatMessageStore, call_id: &str) -> ToolResultBlock {
        store
            .messages()
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                ChatBlock::ToolResult(result) if result.call_id == call_id => Some(result.clone()),
                _ => None,
            })
            .expect("tool result block should exist")
    }

    fn approval_decision(
        store: &ChatMessageStore,
        block_id: atto_ui_chat::ChatBlockId,
        approval_id: &str,
        option_id: &str,
        action: ApprovalAction,
        level: ApprovalLevel,
    ) -> ApprovalDecision {
        ApprovalDecision {
            message_id: store.messages()[0].id,
            block_id,
            approval_id: approval_id.to_string(),
            option_id: option_id.to_string(),
            action,
            level,
        }
    }

    #[test]
    fn builds_single_chat_window_with_status_bar() {
        let app = AgentApp::new(Rect::new(0, 0, 80, 24));

        let windows = app.desktop().list_windows();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].title, APP_TITLE);
        assert_eq!(windows[0].tag.as_deref(), Some("atto-agent:chat"));
        assert!(windows[0].is_focused);
        assert!(app.desktop().status.has_segments());
        assert_eq!(app.chat_window_id(), windows[0].id);
    }

    #[test]
    fn initializes_chat_store_and_input_handle() {
        let app = AgentApp::new(Rect::new(0, 0, 80, 24));

        assert!(app.message_store().messages().is_empty());
        assert_eq!(app.status_state().get(), STATUS_READY);
        assert_eq!(app.provider_state().get(), AgentProvider::Mock.status());
        assert_eq!(app.model_state().get(), "model: deepseek-chat");
        assert_eq!(app.plan_mode_state().get(), PlanMode::Auto.status());
        assert_eq!(app.tool_count_state().get(), "tools: 5");
        assert_eq!(app.skill_count_state().get(), "skills: 0");
        assert_eq!(app.token_estimate_state().get(), "tokens~0");
        assert_eq!(app.error_summary_state().get(), "err:ok");
        assert!(app.loaded_skills().is_empty());
        match app.input_handle().mode() {
            ChatInputMode::Text(config) => {
                assert_eq!(config.title, "Message");
                assert_eq!(config.placeholder.as_deref(), Some("Type a message..."));
            }
            other => panic!("expected text input mode, got {other:?}"),
        }
    }

    #[test]
    fn startup_notice_explains_implicit_mock_when_deepseek_key_is_missing() {
        let store = ChatMessageStore::new();
        let config = AgentConfig::defaults(".");

        append_startup_notices(&config, &store);

        let messages = store.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, ChatRole::System);
        assert_eq!(message_text(&messages[0]), MISSING_DEEPSEEK_API_KEY_NOTICE);
        assert!(message_text(&messages[0]).contains("DEEPSEEK_API_KEY"));
        assert!(message_text(&messages[0]).contains("--api-key"));
        assert!(message_text(&messages[0]).contains("--mock"));
    }

    #[test]
    fn startup_notice_is_suppressed_for_explicit_mock_or_existing_transcript() {
        let mut forced_mock = AgentConfig::defaults(".");
        forced_mock.force_mock = true;
        let explicit_mock_store = ChatMessageStore::new();
        append_startup_notices(&forced_mock, &explicit_mock_store);
        assert!(explicit_mock_store.messages().is_empty());

        let existing_store = ChatMessageStore::new();
        existing_store.push(ChatMessage::text(
            existing_store.next_message_id(),
            ChatRole::User,
            "existing transcript",
        ));
        append_startup_notices(&AgentConfig::defaults("."), &existing_store);
        assert_eq!(existing_store.messages().len(), 1);
    }

    #[test]
    fn applies_configured_model_and_plan_mode_to_runtime_state() {
        let mut config = AgentConfig::defaults(".");
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.model = "deepseek-reasoner".to_string();
        config.plan_mode = PlanMode::On;

        let app = AgentApp::with_config(Rect::new(0, 0, 80, 24), config);

        assert_eq!(app.config().provider, AgentProvider::DeepSeek);
        assert_eq!(app.provider_state().get(), AgentProvider::DeepSeek.status());
        assert_eq!(app.config().model, "deepseek-reasoner");
        assert_eq!(app.model_state().get(), "model: deepseek-reasoner");
        assert_eq!(app.plan_mode_state().get(), PlanMode::On.status());
    }

    #[test]
    fn status_bar_segments_include_agent_runtime_fields() {
        let model = atto_ui::reactive::Property::new("model: deepseek-chat".to_string());
        let provider = atto_ui::reactive::Property::new(AgentProvider::DeepSeek.status());
        let state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan = atto_ui::reactive::Property::new(PlanMode::Auto.status());
        let tools = atto_ui::reactive::Property::new("tools: 5".to_string());
        let skills = atto_ui::reactive::Property::new("skills: 0".to_string());
        let tokens = atto_ui::reactive::Property::new("tokens~0".to_string());
        let error = atto_ui::reactive::Property::new("err:ok".to_string());

        let segments = status_segments(StatusSegmentBindings {
            model: model.binding(),
            provider: provider.binding(),
            state: state.binding(),
            plan_mode: plan.binding(),
            tools: tools.binding(),
            skills: skills.binding(),
            tokens: tokens.binding(),
            error: error.binding(),
        });
        let pairs = segments
            .iter()
            .map(|segment| (segment.id.as_str(), segment.text.get()))
            .collect::<Vec<_>>();

        assert!(pairs.contains(&("model", "model: deepseek-chat".to_string())));
        assert!(pairs.contains(&("provider", "provider: deepseek".to_string())));
        assert!(pairs.contains(&("plan", "plan: auto".to_string())));
        assert!(pairs.contains(&("tools", "tools: 5".to_string())));
        assert!(pairs.contains(&("skills", "skills: 0".to_string())));
        assert!(pairs.contains(&("tokens", "tokens~0".to_string())));
        assert!(pairs.contains(&("error", "err:ok".to_string())));
        assert!(pairs.contains(&("streaming", STATUS_READY.to_string())));
    }

    #[test]
    fn transcript_status_summarizes_tokens_and_latest_error() {
        let store = ChatMessageStore::new();
        let token_estimate_state =
            atto_ui::reactive::Property::new(format_token_estimate_status(0));
        let error_summary_state = atto_ui::reactive::Property::new(error_summary_status(&[]));

        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "hello status tokens",
        ));
        sync_transcript_status(&store, &token_estimate_state, &error_summary_state);

        assert_ne!(token_estimate_state.get(), "tokens~0");
        assert_eq!(error_summary_state.get(), "err:ok");

        store.push(
            ChatMessage::text(store.next_message_id(), ChatRole::Assistant, "").with_status(
                ChatTurnStatus::Failed(ChatError::new(
                    ChatErrorKind::Network,
                    "Network stream disconnected while reading SSE",
                )),
            ),
        );
        sync_transcript_status(&store, &token_estimate_state, &error_summary_state);

        assert!(error_summary_state.get().starts_with("err:network"));
    }

    #[test]
    fn transcript_persistence_debounces_dirty_saves_and_flushes_on_save_now() {
        let workspace = unique_temp_dir("transcript-debounce");
        fs::create_dir_all(&workspace).expect("create workspace");
        let transcript_path = workspace.join("session.jsonl");
        let store = ChatMessageStore::new();
        let mut persistence = TranscriptPersistence::new(Some(transcript_path.clone()), &store);

        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "first persisted message",
        ));
        persistence
            .save_if_dirty()
            .expect("initial save should pass");
        let saved = fs::read_to_string(&transcript_path).expect("read initial transcript");
        assert!(saved.contains("first persisted message"));

        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "second pending message",
        ));
        persistence
            .save_if_dirty()
            .expect("debounced save should pass");
        let debounced = fs::read_to_string(&transcript_path).expect("read debounced transcript");
        assert!(!debounced.contains("second pending message"));

        persistence.save_now().expect("final flush should pass");
        let flushed = fs::read_to_string(&transcript_path).expect("read flushed transcript");
        assert!(flushed.contains("second pending message"));

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn injects_submit_slash_commands() {
        let app = AgentApp::new(Rect::new(0, 0, 80, 24));
        let commands = app.input_handle().slash_commands();
        let labels = commands
            .iter()
            .map(|command| command.label.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "/help", "/clear", "/plan", "/skills", "/skill", "/tools", "/abort"
            ]
        );
        assert!(
            commands
                .iter()
                .all(|command| command.action == ChatSlashCommandAction::Submit)
        );
    }

    #[test]
    fn help_slash_command_outputs_available_commands_without_starting_turn() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let turn_budgets = TurnBudgetTracker::default();

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/help",
        ));

        let messages = store.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, ChatRole::System);
        assert!(message_text(&messages[0]).contains("/clear"));
        assert!(message_text(&messages[0]).contains("/skill <name>"));
        assert!(message_text(&messages[0]).contains("/abort"));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn clear_slash_command_removes_messages_and_resets_runtime_state() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::On.status());
        let turn_budgets = TurnBudgetTracker::default();
        input_handle.streaming_binding().set(true);
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "seed",
        ));
        let assistant_id = store.next_message_id();
        store.push(
            ChatMessage::text(assistant_id, ChatRole::Assistant, "partial")
                .with_status(ChatTurnStatus::Streaming),
        );
        let cancel = mock_turns.start(assistant_id);

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/clear",
        ));

        assert!(store.messages().is_empty());
        assert!(cancel.is_cancelled());
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert_eq!(plan_mode_state.get(), PlanMode::On.status());
    }

    #[test]
    fn plan_slash_command_sets_and_cycles_plan_mode() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let turn_budgets = TurnBudgetTracker::default();

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/plan on",
        ));
        assert_eq!(plan_mode_state.get(), PlanMode::On.status());

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/plan auto",
        ));
        assert_eq!(plan_mode_state.get(), PlanMode::Auto.status());

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/plan",
        ));
        assert_eq!(plan_mode_state.get(), PlanMode::Off.status());

        let messages = store.messages();
        assert!(message_text(&messages[0]).contains("Plan mode set to on."));
        assert!(message_text(&messages[1]).contains("Plan mode set to auto."));
        assert!(message_text(&messages[2]).contains("Plan mode set to off."));
    }

    #[test]
    fn skills_and_tools_slash_commands_report_current_registries() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let turn_budgets = TurnBudgetTracker::default();

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/skills",
        ));
        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/tools",
        ));

        let messages = store.messages();
        assert!(message_text(&messages[0]).contains("Skills: 0 discovered, 0 loaded"));
        assert!(message_text(&messages[0]).contains("No skills found"));
        assert!(message_text(&messages[0]).contains("No skills loaded"));
        assert!(message_text(&messages[1]).contains("Tools: 5 registered"));
        assert!(message_text(&messages[1]).contains("apply_patch"));
        assert!(message_text(&messages[1]).contains("read_file"));
        assert!(message_text(&messages[1]).contains("list_files"));
        assert!(message_text(&messages[1]).contains("run_command"));
        assert!(message_text(&messages[1]).contains("search_text"));
    }

    #[test]
    fn skill_slash_command_activates_skill_and_updates_listing() {
        let (workspace, registry) = test_skill_registry(&[
            ("rust", "rust-review", "Review Rust code."),
            ("docs", "docs", "Write documentation."),
        ]);
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let skills = TestSkillState::new(registry);
        let turn_budgets = TurnBudgetTracker::default();
        let runtime = test_slash_runtime_with_skills(
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            &skills,
            &turn_budgets,
        );

        assert!(submit_slash_command_text(
            &store,
            &runtime,
            "/skill rust-review"
        ));
        assert!(skills.loaded.contains("rust-review"));
        assert_eq!(skills.loaded.names(), vec!["rust-review"]);
        assert_eq!(skills.count_state.get(), "skills: 1");

        assert!(submit_slash_command_text(
            &store,
            &runtime,
            "/skill rust-review"
        ));
        assert_eq!(skills.loaded.names(), vec!["rust-review"]);
        assert_eq!(skills.count_state.get(), "skills: 1");

        assert!(submit_slash_command_text(&store, &runtime, "/skills"));

        let messages = store.messages();
        assert!(message_text(&messages[0]).contains("Loaded skill `rust-review`"));
        assert!(message_text(&messages[1]).contains("already active"));
        assert!(message_text(&messages[2]).contains("Skills: 2 discovered, 1 loaded"));
        assert!(message_text(&messages[2]).contains("- [available] docs"));
        assert!(message_text(&messages[2]).contains("- [loaded] rust-review"));
        assert!(message_text(&messages[2]).contains("Loaded skills: rust-review."));

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn skill_slash_command_reports_usage_and_unknown_skill() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let skills = TestSkillState::default();
        let turn_budgets = TurnBudgetTracker::default();
        let runtime = test_slash_runtime_with_skills(
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            &skills,
            &turn_budgets,
        );

        assert!(submit_slash_command_text(&store, &runtime, "/skill"));
        assert!(submit_slash_command_text(
            &store,
            &runtime,
            "/skill missing"
        ));

        let messages = store.messages();
        assert!(message_text(&messages[0]).contains("Usage: /skill <name>"));
        assert!(message_text(&messages[1]).contains("Skill `missing` not found"));
        assert!(skills.loaded.is_empty());
        assert_eq!(skills.count_state.get(), "skills: 0");
    }

    #[test]
    fn abort_slash_command_cancels_latest_streaming_turn_and_rejects_late_tokens() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let turn_budgets = TurnBudgetTracker::default();
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let block_id = assistant.blocks[0].id();
        store.push(assistant);
        let stale_branch = store.branch_token();
        let cancel = mock_turns.start(assistant_id);

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/abort",
        ));

        let messages = store.messages();
        assert_eq!(messages[0].status, ChatTurnStatus::Canceled);
        assert_eq!(messages[1].role, ChatRole::System);
        assert!(message_text(&messages[1]).contains("Aborted active turn."));
        assert!(cancel.is_cancelled());
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(!apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TextDelta {
                branch: stale_branch,
                block_id,
                delta: "late".to_string(),
            },
        ));
        assert_eq!(message_text(&store.messages()[0]), "");
    }

    #[test]
    fn esc_cancel_through_chat_panel_cancels_mock_turn_and_rejects_late_tokens() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let block_id = assistant.blocks[0].id();
        store.push(assistant);
        let stale_branch = store.branch_token();
        let cancel = mock_turns.start(assistant_id);
        let mut panel = build_chat_panel(
            &store,
            AgentTurnLauncher {
                config: AgentConfig::defaults("."),
                action_sender: sender.clone(),
                tool_registry: test_tool_registry(),
                turn_budgets: turn_budgets.clone(),
                limits: AgentTurnLimits::default(),
                compact_policy: CompactPolicy::default(),
            },
            test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            test_tool_runtime(
                AgentConfig::defaults("."),
                sender,
                test_tool_registry(),
                test_tool_permissions(),
            ),
            atto_ui::reactive::EventQueue::new(),
        );
        let theme = Theme::dark();

        let result = panel.handle_event(
            &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            context(&theme),
        );

        assert!(result.is_consumed());
        assert!(cancel.is_cancelled());
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert_eq!(store.messages()[0].status, ChatTurnStatus::Canceled);
        assert!(!apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TextDelta {
                branch: stale_branch,
                block_id,
                delta: "late".to_string(),
            },
        ));
        assert_eq!(message_text(&store.messages()[0]), "");
    }

    #[test]
    fn text_submit_adds_user_and_streaming_assistant_turn() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let turn_launcher = AgentTurnLauncher {
            config: AgentConfig::defaults("."),
            action_sender: sender,
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
        };

        submit_input_response(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            &turn_launcher,
            ChatInputResponse::Text("hello".to_string()),
        );

        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(message_text(&messages[0]), "hello");
        assert_eq!(messages[1].role, ChatRole::Assistant);
        assert_eq!(message_text(&messages[1]), "");
        assert_eq!(messages[1].status, ChatTurnStatus::Streaming);
        assert!(input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_STREAMING);
        drop(receiver);
    }

    #[test]
    fn deepseek_provider_streams_live_events_through_app_actions() {
        let server = TestSseServer::spawn(concat!(
            "data: {\"model\":\"mock-deepseek\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello from live\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        ));
        let mut config = AgentConfig::defaults(".");
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;

        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender.clone(),
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
        };
        let tool_runtime = test_tool_runtime(
            config.clone(),
            sender,
            test_tool_registry(),
            test_tool_permissions(),
        );
        let transcript_status = TranscriptStatusState::new();

        submit_input_response(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            &turn_launcher,
            ChatInputResponse::Text("live prompt".to_string()),
        );

        for _ in 0..4 {
            let action = receiver
                .recv_timeout(Duration::from_secs(2))
                .expect("live DeepSeek turn should emit an app action");
            apply_app_action(
                &store,
                &input_handle,
                &mock_turns,
                &status_state,
                &transcript_status,
                &tool_runtime,
                action,
            );
            if !input_handle.streaming_binding().get() {
                break;
            }
        }

        let request = server.join();
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(request.contains(r#""content":"live prompt""#));
        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[1].role, ChatRole::Assistant);
        assert_eq!(message_text(&messages[1]), "hello from live");
        assert_eq!(messages[1].status, ChatTurnStatus::Complete);
        assert_eq!(messages[1].meta.model.as_deref(), Some("mock-deepseek"));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn deepseek_provider_abort_slash_cancels_in_flight_http_request_and_rejects_late_events() {
        let server = TestAbortableSseServer::spawn(
            "data: {\"model\":\"mock-deepseek\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"first live token\"},\"finish_reason\":null}]}\n\n",
        );
        let mut config = AgentConfig::defaults(".");
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;

        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender.clone(),
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
        };
        let tool_runtime = test_tool_runtime(
            config,
            sender,
            test_tool_registry(),
            test_tool_permissions(),
        );
        let transcript_status = TranscriptStatusState::new();

        submit_input_response(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            &turn_launcher,
            ChatInputResponse::Text("live cancel prompt".to_string()),
        );
        server.wait_for_first_event();

        let action = receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("live DeepSeek turn should emit the first token");
        assert!(apply_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
            action,
        ));
        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(message_text(&messages[1]), "first live token");
        assert_eq!(messages[1].status, ChatTurnStatus::Streaming);
        let assistant_id = messages[1].id;
        let block_id = messages[1].blocks[0].id();
        let stale_branch = store.branch_token();

        assert!(submit_slash_command_text(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            "/abort",
        ));

        let messages = store.messages();
        assert_eq!(messages[1].id, assistant_id);
        assert_eq!(messages[1].status, ChatTurnStatus::Canceled);
        assert_eq!(messages[2].role, ChatRole::System);
        assert!(message_text(&messages[2]).contains("Aborted active turn."));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);

        let (request, client_closed) = server.join();
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(
            client_closed,
            "aborting a live turn should close the in-flight SSE connection"
        );
        assert!(!apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TextDelta {
                branch: stale_branch,
                block_id,
                delta: "late".to_string(),
            },
        ));
        assert_eq!(message_text(&store.messages()[1]), "first live token");
    }

    #[test]
    fn deepseek_provider_missing_api_key_fails_turn_with_actionable_error() {
        let mut config = AgentConfig::defaults(".");
        config.provider = AgentProvider::DeepSeek;
        config.plan_mode = PlanMode::Off;

        let result = run_live_prompt_to_idle(config, "live without key");

        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].role, ChatRole::User);
        let error = failed_turn_error(&result.messages[1]);
        assert_eq!(error.kind, ChatErrorKind::Api);
        assert!(error.message.contains("DEEPSEEK_API_KEY"));
        assert!(error.message.contains("--api-key"));
        assert!(error.message.contains("--mock"));
        let detail = error.detail.as_deref().expect("detail should be present");
        assert!(detail.contains("DEEPSEEK_API_KEY"));
        assert!(detail.contains("--api-key"));
        assert_eq!(result.status, STATUS_READY);
        assert!(!result.streaming);
        assert!(result.error_summary.starts_with("err:api"));
    }

    #[test]
    fn deepseek_provider_http_errors_reuse_structured_chat_error_mapping() {
        let auth = live_http_error(
            401,
            "Unauthorized",
            r#"{"error":{"message":"bad key","type":"invalid_request_error","code":"invalid_api_key","param":null}}"#,
        );
        assert!(
            auth.request
                .starts_with("POST /v1/chat/completions HTTP/1.1")
        );
        assert_eq!(auth.error.kind, ChatErrorKind::Api);
        assert!(auth.error.message.contains("DEEPSEEK_API_KEY"));
        assert!(auth.error.detail.as_deref().is_some_and(|detail| {
            detail.contains("HTTP status: 401") && detail.contains("invalid_api_key")
        }));
        assert!(auth.error_summary.starts_with("err:api"));

        let rate_limit = live_http_error(429, "Too Many Requests", "rate limit body");
        assert_eq!(rate_limit.error.kind, ChatErrorKind::RateLimit);
        assert!(rate_limit.error.message.contains("429"));
        assert!(
            rate_limit
                .error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("rate limit body"))
        );
        assert!(rate_limit.error_summary.starts_with("err:rate"));

        let service = live_http_error(502, "Bad Gateway", "gateway down");
        assert_eq!(service.error.kind, ChatErrorKind::Api);
        assert!(service.error.message.contains("502"));
        assert!(
            service
                .error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("gateway down"))
        );
        assert!(service.error_summary.starts_with("err:api"));
    }

    #[test]
    fn deepseek_provider_stream_disconnect_fails_turn_with_network_error() {
        let server = TestSseServer::spawn(
            "data: {\"model\":\"mock-deepseek\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial live token\"},\"finish_reason\":null}]}\n\n",
        );
        let mut config = AgentConfig::defaults(".");
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;

        let result = run_live_prompt_to_idle(config, "live disconnect prompt");
        let request = server.join();

        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert_eq!(result.messages.len(), 2);
        assert_eq!(message_text(&result.messages[1]), "partial live token");
        let error = failed_turn_error(&result.messages[1]);
        assert_eq!(error.kind, ChatErrorKind::Network);
        assert!(error.message.contains("network stream failed"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("[DONE]"))
        );
        assert_eq!(result.status, STATUS_READY);
        assert!(!result.streaming);
        assert!(result.error_summary.starts_with("err:network"));
    }

    #[test]
    fn deepseek_provider_posts_context_builder_request_with_tools() {
        let workspace = unique_temp_dir("live-context-request");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(workspace.join("note.txt"), "workspace note context\n").expect("write note");
        write_test_skill(
            &workspace,
            "rust",
            "rust-review",
            "Review Rust code before responding.",
        );
        let skill_registry =
            SkillRegistry::discover_from_paths(&[SkillSearchPath::workspace(&workspace)]);
        let skills = TestSkillState::new(skill_registry);
        assert!(skills.loaded.insert("rust-review"));

        let server = TestSseServer::spawn("data: [DONE]\n\n");
        let mut config = AgentConfig::defaults(workspace.clone());
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;
        let registry = test_tool_registry();

        let store = ChatMessageStore::new();
        store.push(ChatMessage::new(
            store.next_message_id(),
            ChatRole::System,
            vec![ChatBlock::Compact(CompactBlock {
                id: ChatBlockId::new(70_001),
                status: CompactStatus::Complete,
                before_tokens: Some(2048),
                after_tokens: Some(256),
                summary: "summarized earlier conversation".to_string(),
            })],
        ));
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "Earlier request",
        ));
        store.push(ChatMessage::new(
            store.next_message_id(),
            ChatRole::Assistant,
            vec![
                ChatBlock::ToolUse(read_file_tool_call("call_read", "prior.txt")),
                ChatBlock::ToolResult(ToolResultBlock {
                    id: ChatBlockId::new(70_002),
                    call_id: "call_read".to_string(),
                    ok: true,
                    exit_code: None,
                    output: ToolOutput::Markdown("prior tool output".to_string()),
                    collapsed: false,
                }),
            ],
        ));

        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let slash_runtime = test_slash_runtime_with_skills(
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            &skills,
            &turn_budgets,
        );
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender,
            tool_registry: registry.clone(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
        };

        submit_input_response(
            &store,
            &slash_runtime,
            &turn_launcher,
            ChatInputResponse::Text("Use @note.txt with rust-review context".to_string()),
        );

        let request = http_request_json(&server.join());
        let messages = request["messages"]
            .as_array()
            .expect("request should contain messages");
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("system")
                && message["content"].as_str().is_some_and(|content| {
                    content.contains("<skills>")
                        && content.contains("rust-review")
                        && content.contains("Use this skill for rust-review tasks.")
                })
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("system")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("<compact status=\"complete\""))
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("user")
                && message["content"].as_str().is_some_and(|content| {
                    content.contains("Use @note.txt")
                        && content.contains("<context_files>")
                        && content.contains("workspace note context")
                })
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("assistant")
                && message["tool_calls"]
                    .as_array()
                    .is_some_and(|calls| calls[0]["function"]["name"].as_str() == Some("read_file"))
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("tool")
                && message["tool_call_id"].as_str() == Some("call_read")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("prior tool output"))
        }));

        let tool_names = request["tools"]
            .as_array()
            .expect("request should include tool schema")
            .iter()
            .filter_map(|tool| tool["function"]["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(tool_names.len(), registry.len());
        assert!(tool_names.contains(&"read_file"));
        assert!(tool_names.contains(&"run_command"));
        assert_eq!(request["tool_choice"].as_str(), Some("auto"));

        drop(receiver);
        fs::remove_dir_all(workspace).expect("remove workspace");
    }

    #[test]
    fn deepseek_provider_continues_after_live_tool_result() {
        let workspace = unique_temp_dir("live-tool-loop");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(workspace.join("note.txt"), "tool loop file context\n").expect("write note");
        let server = TestSseSequenceServer::spawn(vec![
            sse_tool_call_body(
                "call_read_note",
                "read_file",
                serde_json::json!({ "path": "note.txt" }),
            ),
            sse_final_text_body("Final answer after reading the file."),
        ]);
        let mut config = AgentConfig::defaults(workspace.clone());
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;
        let registry = test_tool_registry();
        let limits = AgentTurnLimits::default();
        let turn_budgets = TurnBudgetTracker::default();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let transcript_status = TranscriptStatusState::new();
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender.clone(),
            tool_registry: registry.clone(),
            turn_budgets: turn_budgets.clone(),
            limits,
            compact_policy: CompactPolicy::default(),
        };
        let tool_runtime = live_tool_runtime(LiveToolRuntimeParts {
            config,
            action_sender: sender,
            registry,
            turn_budgets: turn_budgets.clone(),
            limits,
            input_handle: &input_handle,
            mock_turns: &mock_turns,
            status_state: &status_state,
            transcript_status: &transcript_status,
        });

        submit_input_response(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            &turn_launcher,
            ChatInputResponse::Text("Read the note before answering.".to_string()),
        );
        apply_live_actions_until_idle(
            &receiver,
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
        );

        let requests = server.join();
        assert_eq!(requests.len(), 2);
        let second_request = http_request_json(&requests[1]);
        let second_messages = second_request["messages"]
            .as_array()
            .expect("follow-up request should contain messages");
        assert!(second_messages.iter().any(|message| {
            message["role"].as_str() == Some("assistant")
                && message["tool_calls"]
                    .as_array()
                    .is_some_and(|calls| calls[0]["id"].as_str() == Some("call_read_note"))
        }));
        assert!(second_messages.iter().any(|message| {
            message["role"].as_str() == Some("tool")
                && message["tool_call_id"].as_str() == Some("call_read_note")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("tool loop file context"))
        }));

        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        let assistant = &messages[1];
        assert_eq!(assistant.status, ChatTurnStatus::Complete);
        assert!(assistant.blocks.iter().any(|block| {
            matches!(block, ChatBlock::Text(text) if text.markdown.contains("Final answer after reading the file."))
        }));
        let tool = assistant
            .blocks
            .iter()
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) if tool.call_id == "call_read_note" => Some(tool),
                _ => None,
            })
            .expect("tool use should remain in the transcript");
        assert_eq!(tool.status, ToolStatus::Done);
        assert!(tool_result_for_call(&store, "call_read_note").ok);
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        fs::remove_dir_all(workspace).expect("remove workspace");
    }

    #[test]
    fn deepseek_provider_continues_after_denied_live_tool_call() {
        let workspace = unique_temp_dir("live-tool-loop-deny");
        fs::create_dir_all(&workspace).expect("create workspace");
        let server = TestSseSequenceServer::spawn(vec![
            sse_tool_call_body(
                "call_denied_command",
                "run_command",
                serde_json::json!({ "argv": ["/bin/echo", "should-not-run"], "cwd": "." }),
            ),
            sse_final_text_body("Final answer after the denied tool."),
        ]);
        let mut config = AgentConfig::defaults(workspace.clone());
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;
        let registry = test_tool_registry();
        let limits = AgentTurnLimits::default();
        let turn_budgets = TurnBudgetTracker::default();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let transcript_status = TranscriptStatusState::new();
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender.clone(),
            tool_registry: registry.clone(),
            turn_budgets: turn_budgets.clone(),
            limits,
            compact_policy: CompactPolicy::default(),
        };
        let tool_runtime = live_tool_runtime(LiveToolRuntimeParts {
            config,
            action_sender: sender,
            registry,
            turn_budgets: turn_budgets.clone(),
            limits,
            input_handle: &input_handle,
            mock_turns: &mock_turns,
            status_state: &status_state,
            transcript_status: &transcript_status,
        });

        submit_input_response(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            &turn_launcher,
            ChatInputResponse::Text("Try a command, but wait for approval.".to_string()),
        );
        let action = receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("tool call should be streamed before approval");
        assert!(apply_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
            action,
        ));
        assert!(input_handle.streaming_binding().get());
        let assistant_id = store.messages()[1].id;
        let tool = store
            .messages()
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) if tool.call_id == "call_denied_command" => {
                    Some(tool.clone())
                }
                _ => None,
            })
            .expect("tool approval should be pending");
        assert_eq!(tool.status, ToolStatus::Pending);
        let approval_id = tool
            .approval
            .as_ref()
            .expect("run_command should request approval")
            .id
            .clone();

        handle_tool_approval(
            &store,
            &tool_runtime,
            ApprovalDecision {
                message_id: assistant_id,
                block_id: tool.id,
                approval_id,
                option_id: "deny".to_string(),
                action: ApprovalAction::Deny,
                level: ApprovalLevel::Once,
            },
        );
        apply_live_actions_until_idle(
            &receiver,
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
        );

        let requests = server.join();
        assert_eq!(requests.len(), 2);
        let second_request = http_request_json(&requests[1]);
        let second_messages = second_request["messages"]
            .as_array()
            .expect("follow-up request should contain messages");
        assert!(second_messages.iter().any(|message| {
            message["role"].as_str() == Some("tool")
                && message["tool_call_id"].as_str() == Some("call_denied_command")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("User denied tool call"))
        }));
        let result = tool_result_for_call(&store, "call_denied_command");
        assert!(!result.ok);
        let messages = store.messages();
        let assistant = &messages[1];
        assert_eq!(assistant.status, ChatTurnStatus::Complete);
        assert!(assistant.blocks.iter().any(|block| {
            matches!(block, ChatBlock::Text(text) if text.markdown.contains("Final answer after the denied tool."))
        }));
        fs::remove_dir_all(workspace).expect("remove workspace");
    }

    #[test]
    fn deepseek_provider_stops_tool_loop_at_model_request_budget() {
        let workspace = unique_temp_dir("live-tool-loop-budget");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(workspace.join("note.txt"), "budget file context\n").expect("write note");
        let server = TestSseServer::spawn(sse_tool_call_body(
            "call_read_budget",
            "read_file",
            serde_json::json!({ "path": "note.txt" }),
        ));
        let mut config = AgentConfig::defaults(workspace.clone());
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::Off;
        let registry = test_tool_registry();
        let limits = AgentTurnLimits::new(1, 16, Duration::from_secs(30));
        let turn_budgets = TurnBudgetTracker::default();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let transcript_status = TranscriptStatusState::new();
        let turn_launcher = AgentTurnLauncher {
            config: config.clone(),
            action_sender: sender.clone(),
            tool_registry: registry.clone(),
            turn_budgets: turn_budgets.clone(),
            limits,
            compact_policy: CompactPolicy::default(),
        };
        let tool_runtime = live_tool_runtime(LiveToolRuntimeParts {
            config,
            action_sender: sender,
            registry,
            turn_budgets: turn_budgets.clone(),
            limits,
            input_handle: &input_handle,
            mock_turns: &mock_turns,
            status_state: &status_state,
            transcript_status: &transcript_status,
        });

        submit_input_response(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            &turn_launcher,
            ChatInputResponse::Text("Read the note within a tiny budget.".to_string()),
        );
        apply_live_actions_until_idle(
            &receiver,
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
        );

        let first_request = server.join();
        assert!(first_request.contains("Read the note within a tiny budget."));
        let messages = store.messages();
        let assistant = &messages[1];
        let ChatTurnStatus::Failed(error) = &assistant.status else {
            panic!("expected failed assistant turn, got {:?}", assistant.status);
        };
        assert_eq!(error.kind, ChatErrorKind::Other);
        assert!(error.message.contains("model request limit"));
        assert!(tool_result_for_call(&store, "call_read_budget").ok);
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        fs::remove_dir_all(workspace).expect("remove workspace");
    }

    #[test]
    fn deepseek_provider_plan_turn_posts_submit_plan_context_request() {
        let workspace = unique_temp_dir("live-plan-request");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(workspace.join("plan.txt"), "plan context\n").expect("write note");
        let server = TestSseServer::spawn("data: [DONE]\n\n");
        let mut config = AgentConfig::defaults(workspace.clone());
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::On;

        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::On.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let turn_launcher = AgentTurnLauncher {
            config,
            action_sender: sender,
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
        };

        submit_input_response(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            &turn_launcher,
            ChatInputResponse::Text("Update @plan.txt and run tests".to_string()),
        );

        let request = http_request_json(&server.join());
        let messages = request["messages"]
            .as_array()
            .expect("request should contain messages");
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("system")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("You are in plan mode"))
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("user")
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("plan context"))
        }));
        let tools = request["tools"]
            .as_array()
            .expect("plan request should include virtual submit_plan tool");
        assert_eq!(tools.len(), 1);
        assert_eq!(
            tools[0]["function"]["name"].as_str(),
            Some(crate::plan::SUBMIT_PLAN_TOOL_NAME)
        );
        assert_eq!(
            request["tool_choice"]["function"]["name"].as_str(),
            Some(crate::plan::SUBMIT_PLAN_TOOL_NAME)
        );
        drop(receiver);
        fs::remove_dir_all(workspace).expect("remove workspace");
    }

    #[test]
    fn deepseek_provider_accepted_plan_continue_posts_transcript_request_with_tools() {
        let server = TestSseServer::spawn("data: [DONE]\n\n");
        let mut config = AgentConfig::defaults(".");
        config.api_key = Some("test-key".to_string());
        config.provider = AgentProvider::DeepSeek;
        config.base_url = server.base_url();
        config.plan_mode = PlanMode::On;

        let store = ChatMessageStore::new();
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "Please update the implementation.",
        ));
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let turn_budgets = TurnBudgetTracker::default();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let registry = test_tool_registry();
        let runtime = PlanDecisionRuntime {
            input_handle: input_handle.clone(),
            mock_turns: mock_turns.clone(),
            status_state: status_state.clone(),
            skill_registry: SkillRegistry::default(),
            loaded_skills: LoadedSkillSet::default(),
            transcript_status: TranscriptStatusState::new(),
            turn_launcher: AgentTurnLauncher {
                config,
                action_sender: sender,
                tool_registry: registry.clone(),
                turn_budgets: turn_budgets.clone(),
                limits: AgentTurnLimits::default(),
                compact_policy: CompactPolicy::default(),
            },
        };
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let plan_block_id = ChatBlockId::new(70_003);
        store.push(
            ChatMessage::new(
                assistant_id,
                ChatRole::Assistant,
                vec![ChatBlock::Plan(PlanBlock {
                    id: plan_block_id,
                    items: vec![PlanItem {
                        text: "Inspect and edit.".to_string(),
                    }],
                    decision: PlanDecision::Pending,
                })],
            )
            .with_status(ChatTurnStatus::Streaming),
        );
        turn_budgets.start_turn(assistant_id, AgentTurnLimits::default());
        let _plan_cancel = mock_turns.start(assistant_id);

        handle_plan_decision(
            &store,
            &runtime,
            PlanDecisionEvent {
                message_id: assistant_id,
                block_id: plan_block_id,
                decision: PlanDecision::Accepted,
            },
        );

        let request = http_request_json(&server.join());
        let messages = request["messages"]
            .as_array()
            .expect("request should contain messages");
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("user")
                && message["content"].as_str() == Some("Please update the implementation.")
        }));
        assert!(messages.iter().any(|message| {
            message["role"].as_str() == Some("system")
                && message["content"].as_str() == Some(ACCEPTED_PLAN_EXECUTION_INSTRUCTION)
        }));
        let tool_names = request["tools"]
            .as_array()
            .expect("accepted-plan execution should include registered tools")
            .iter()
            .filter_map(|tool| tool["function"]["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(tool_names.len(), registry.len());
        assert!(tool_names.contains(&"apply_patch"));
        assert!(tool_names.contains(&"run_command"));
        assert_eq!(request["tool_choice"].as_str(), Some("auto"));
        assert!(!tool_names.contains(&crate::plan::SUBMIT_PLAN_TOOL_NAME));

        drop(receiver);
    }

    #[test]
    fn text_submit_compacts_older_transcript_before_starting_turn() {
        let store = ChatMessageStore::new();
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "old user zero full body should be summarized",
        ));
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::Assistant,
            "old assistant zero full body should be summarized",
        ));
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::User,
            "old user one full body should be summarized",
        ));
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::Assistant,
            "recent assistant keep",
        ));
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let turn_launcher = AgentTurnLauncher {
            config: AgentConfig::defaults("."),
            action_sender: sender,
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy {
                threshold_tokens: 1,
                recent_message_limit: 2,
                summary_max_bytes: 4096,
            },
        };

        submit_input_response(
            &store,
            &test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            ),
            &turn_launcher,
            ChatInputResponse::Text("current prompt keep".to_string()),
        );

        let messages = store.messages();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, ChatRole::System);
        match &messages[0].blocks[0] {
            ChatBlock::Compact(compact) => {
                assert_eq!(compact.status, CompactStatus::Complete);
                assert!(compact.before_tokens.is_some());
                assert!(compact.after_tokens.is_some());
                assert!(compact.summary.contains("old user zero full body"));
                assert!(compact.summary.contains("old assistant zero full body"));
                assert!(compact.summary.contains("old user one full body"));
            }
            other => panic!("expected compact block, got {other:?}"),
        }
        assert_eq!(message_text(&messages[1]), "recent assistant keep");
        assert_eq!(messages[2].role, ChatRole::User);
        assert_eq!(message_text(&messages[2]), "current prompt keep");
        assert_eq!(messages[3].role, ChatRole::Assistant);
        assert_eq!(messages[3].status, ChatTurnStatus::Streaming);

        let request = deepseek_request_from_transcript(
            &AgentConfig::defaults("."),
            &test_tool_registry(),
            &messages,
        );
        assert_eq!(request.messages.len(), 3);
        assert_eq!(request.messages[0].role, ChatMessageRole::System);
        assert!(
            request.messages[0]
                .content
                .as_deref()
                .is_some_and(|content| content.starts_with("<compact status=\"complete\""))
        );
        assert_eq!(request.messages[1].role, ChatMessageRole::Assistant);
        assert_eq!(
            request.messages[1].content.as_deref(),
            Some("recent assistant keep")
        );
        assert_eq!(request.messages[2].role, ChatMessageRole::User);
        assert_eq!(
            request.messages[2].content.as_deref(),
            Some("current prompt keep")
        );
        assert!(!request.messages.iter().any(|message| {
            message.role == ChatMessageRole::User
                && message
                    .content
                    .as_deref()
                    .is_some_and(|content| content.contains("old user zero full body"))
        }));
        drop(receiver);
    }

    #[test]
    fn text_submit_auto_loads_matching_auto_skills() {
        let workspace = unique_temp_dir("submit-auto-skills");
        write_test_skill_with_mode(
            &workspace,
            "rust",
            "rust-review",
            "Review Rust code.",
            SkillMode::Auto,
            &["clippy"],
        );
        write_test_skill_with_mode(
            &workspace,
            "docs",
            "docs",
            "Write documentation.",
            SkillMode::Manual,
            &["docs"],
        );
        let registry =
            SkillRegistry::discover_from_paths(&[SkillSearchPath::workspace(&workspace)]);
        let skills = TestSkillState::new(registry);
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let turn_budgets = TurnBudgetTracker::default();
        let runtime = test_slash_runtime_with_skills(
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            &skills,
            &turn_budgets,
        );
        let turn_launcher = AgentTurnLauncher {
            config: AgentConfig::defaults("."),
            action_sender: sender,
            tool_registry: test_tool_registry(),
            turn_budgets: turn_budgets.clone(),
            limits: AgentTurnLimits::default(),
            compact_policy: CompactPolicy::default(),
        };

        submit_input_response(
            &store,
            &runtime,
            &turn_launcher,
            ChatInputResponse::Text("please run clippy on this rust code".to_string()),
        );

        assert_eq!(skills.loaded.names(), vec!["rust-review"]);
        assert_eq!(skills.count_state.get(), "skills: 1");
        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[1].role, ChatRole::Assistant);
        drop(receiver);
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn edit_and_resubmit_appends_edited_user_and_restarts_turn() {
        let store = ChatMessageStore::new();
        let user_id = store.next_message_id();
        store.push(ChatMessage::text(user_id, ChatRole::User, "old prompt"));
        store.push(ChatMessage::text(
            store.next_message_id(),
            ChatRole::Assistant,
            "old answer",
        ));
        let removed_messages = store
            .truncate_from(user_id)
            .expect("edit controller should have truncated from the user message");
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
        let turn_budgets = TurnBudgetTracker::default();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let runtime = test_slash_runtime(
            &input_handle,
            &mock_turns,
            &status_state,
            &plan_mode_state,
            &turn_budgets,
        );
        let turn_launcher = test_turn_launcher(sender, &turn_budgets, CompactPolicy::default());

        handle_edit_and_resubmit(
            &store,
            &runtime,
            &turn_launcher,
            EditAndResubmitEvent {
                message_id: user_id,
                original_text: "old prompt".to_string(),
                edited_text: "edited prompt".to_string(),
                removed_messages,
            },
        );

        let messages = store.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(message_text(&messages[0]), "edited prompt");
        assert_eq!(messages[1].role, ChatRole::Assistant);
        assert_eq!(messages[1].status, ChatTurnStatus::Streaming);
        assert!(input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_STREAMING);
        let action = receiver
            .recv_timeout(Duration::from_millis(250))
            .expect("resubmitted edit should start the mock turn");
        assert!(matches!(action, AppAction::TextDelta { .. }));
    }

    #[test]
    fn retry_and_regenerate_restart_from_retained_user_prompt_and_reject_late_tokens() {
        for kind in [MessageActionKind::Retry, MessageActionKind::Regenerate] {
            let store = ChatMessageStore::new();
            store.push(ChatMessage::text(
                store.next_message_id(),
                ChatRole::User,
                "retry prompt",
            ));
            let assistant_id = store.next_message_id();
            let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "old")
                .with_status(ChatTurnStatus::Streaming);
            let old_text_block_id = assistant.blocks[0].id();
            store.push(assistant);
            let stale_branch = store.branch_token();
            let input_handle = atto_ui_chat::ChatInputHandle::new();
            let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
            let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
            let plan_mode_state = atto_ui::reactive::Property::new(PlanMode::Off.status());
            let turn_budgets = TurnBudgetTracker::default();
            input_handle.streaming_binding().set(true);
            turn_budgets.start_turn(assistant_id, AgentTurnLimits::default());
            let old_cancel = mock_turns.start(assistant_id);
            let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
            let runtime = test_slash_runtime(
                &input_handle,
                &mock_turns,
                &status_state,
                &plan_mode_state,
                &turn_budgets,
            );
            let turn_launcher = test_turn_launcher(sender, &turn_budgets, CompactPolicy::default());
            assert!(store.truncate_from(assistant_id).is_some());

            handle_message_action(
                &store,
                &runtime,
                &turn_launcher,
                MessageAction {
                    message_id: assistant_id,
                    kind,
                },
            );

            assert!(old_cancel.is_cancelled());
            let messages = store.messages();
            assert_eq!(messages.len(), 2);
            assert_eq!(messages[0].role, ChatRole::User);
            assert_eq!(message_text(&messages[0]), "retry prompt");
            assert_eq!(messages[1].role, ChatRole::Assistant);
            assert_eq!(messages[1].status, ChatTurnStatus::Streaming);
            assert!(input_handle.streaming_binding().get());
            assert_eq!(status_state.get(), STATUS_STREAMING);
            assert!(!apply_test_app_action(
                &store,
                &input_handle,
                &mock_turns,
                &status_state,
                AppAction::TextDelta {
                    branch: stale_branch,
                    block_id: old_text_block_id,
                    delta: "late".to_string(),
                },
            ));

            let _first = receiver
                .recv_timeout(Duration::from_millis(250))
                .expect("retry should start streaming");
            let second = receiver
                .recv_timeout(Duration::from_millis(250))
                .expect("retry should stream the retained prompt");
            match second {
                AppAction::TextDelta { delta, .. } => assert_eq!(delta, "retry prompt"),
                other => panic!("expected prompt text delta, got {other:?}"),
            }
        }
    }

    #[test]
    fn app_actions_append_streaming_text_and_complete_turn() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();
        let cancel = mock_turns.start(assistant_id);

        assert!(apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TextDelta {
                branch,
                block_id,
                delta: "Mock ".to_string(),
            },
        ));
        assert!(apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TextDelta {
                branch,
                block_id,
                delta: "done".to_string(),
            },
        ));
        assert!(apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TurnDone {
                branch,
                message_id: assistant_id,
                meta: None,
            },
        ));

        let messages = store.messages();
        assert_eq!(message_text(&messages[0]), "Mock done");
        assert_eq!(messages[0].status, ChatTurnStatus::Complete);
        assert!(!cancel.is_cancelled());
        assert!(!mock_turns.cancel(assistant_id));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn plan_decision_callback_updates_pending_plan_block() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
        let status_state = atto_ui::reactive::Property::new(STATUS_READY.to_string());
        let turn_budgets = TurnBudgetTracker::default();
        let (runtime, _receiver) =
            test_plan_decision_runtime(&input_handle, &mock_turns, &status_state, &turn_budgets);
        let assistant_id = store.next_message_id();
        let plan_block_id = ChatBlockId::new(30_001);
        store.push(ChatMessage::new(
            assistant_id,
            ChatRole::Assistant,
            vec![ChatBlock::Plan(PlanBlock {
                id: plan_block_id,
                items: vec![PlanItem {
                    text: "Inspect current implementation.".to_string(),
                }],
                decision: PlanDecision::Pending,
            })],
        ));

        handle_plan_decision(
            &store,
            &runtime,
            PlanDecisionEvent {
                message_id: assistant_id,
                block_id: plan_block_id,
                decision: PlanDecision::Accepted,
            },
        );

        assert_eq!(plan_decision(&store, plan_block_id), PlanDecision::Accepted);

        handle_plan_decision(
            &store,
            &runtime,
            PlanDecisionEvent {
                message_id: assistant_id,
                block_id: plan_block_id,
                decision: PlanDecision::Rejected,
            },
        );

        assert_eq!(plan_decision(&store, plan_block_id), PlanDecision::Accepted);
    }

    #[test]
    fn accepting_plan_appends_internal_instruction_and_starts_execution_turn() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let turn_budgets = TurnBudgetTracker::default();
        input_handle.streaming_binding().set(true);
        let (runtime, receiver) =
            test_plan_decision_runtime(&input_handle, &mock_turns, &status_state, &turn_budgets);
        let assistant_id = store.next_message_id();
        let plan_block_id = ChatBlockId::new(30_002);
        store.push(
            ChatMessage::new(
                assistant_id,
                ChatRole::Assistant,
                vec![ChatBlock::Plan(PlanBlock {
                    id: plan_block_id,
                    items: vec![PlanItem {
                        text: "Inspect current implementation.".to_string(),
                    }],
                    decision: PlanDecision::Pending,
                })],
            )
            .with_status(ChatTurnStatus::Streaming),
        );
        turn_budgets.start_turn(assistant_id, AgentTurnLimits::default());
        let plan_cancel = mock_turns.start(assistant_id);

        handle_plan_decision(
            &store,
            &runtime,
            PlanDecisionEvent {
                message_id: assistant_id,
                block_id: plan_block_id,
                decision: PlanDecision::Accepted,
            },
        );

        let messages = store.messages();
        assert_eq!(plan_decision(&store, plan_block_id), PlanDecision::Accepted);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].status, ChatTurnStatus::Complete);
        assert!(plan_cancel.is_cancelled());
        assert_eq!(messages[1].role, ChatRole::System);
        assert_eq!(
            message_text(&messages[1]),
            ACCEPTED_PLAN_EXECUTION_INSTRUCTION
        );
        assert_eq!(messages[2].role, ChatRole::Assistant);
        assert_eq!(messages[2].status, ChatTurnStatus::Streaming);
        assert!(input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_STREAMING);
        let action = receiver
            .recv_timeout(Duration::from_millis(250))
            .expect("accepted plan should continue the mock execution loop");
        assert!(matches!(action, AppAction::TextDelta { .. }));
    }

    #[test]
    fn rejecting_plan_stops_turn_without_starting_execution() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::with_token_delay(Duration::from_millis(1));
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let turn_budgets = TurnBudgetTracker::default();
        input_handle.streaming_binding().set(true);
        let (runtime, receiver) =
            test_plan_decision_runtime(&input_handle, &mock_turns, &status_state, &turn_budgets);
        let assistant_id = store.next_message_id();
        let plan_block_id = ChatBlockId::new(30_003);
        store.push(
            ChatMessage::new(
                assistant_id,
                ChatRole::Assistant,
                vec![ChatBlock::Plan(PlanBlock {
                    id: plan_block_id,
                    items: vec![PlanItem {
                        text: "Inspect current implementation.".to_string(),
                    }],
                    decision: PlanDecision::Pending,
                })],
            )
            .with_status(ChatTurnStatus::Streaming),
        );
        turn_budgets.start_turn(assistant_id, AgentTurnLimits::default());
        let plan_cancel = mock_turns.start(assistant_id);

        handle_plan_decision(
            &store,
            &runtime,
            PlanDecisionEvent {
                message_id: assistant_id,
                block_id: plan_block_id,
                decision: PlanDecision::Rejected,
            },
        );

        let messages = store.messages();
        assert_eq!(plan_decision(&store, plan_block_id), PlanDecision::Rejected);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].status, ChatTurnStatus::Complete);
        assert!(plan_cancel.is_cancelled());
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn turn_budget_limits_model_requests() {
        let budgets = TurnBudgetTracker::default();
        let limits = AgentTurnLimits::new(2, 16, std::time::Duration::from_secs(30));
        let message_id = atto_ui_chat::ChatMessageId::new(42);
        budgets.start_turn(message_id, limits);

        assert!(budgets.consume_model_request(message_id, limits).is_ok());
        assert!(budgets.consume_model_request(message_id, limits).is_ok());
        let error = budgets
            .consume_model_request(message_id, limits)
            .expect_err("third model request should exceed the per-turn limit");

        assert_eq!(error.kind, ChatErrorKind::Other);
        assert!(error.message.contains("model request limit"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("per-turn limit is 2"))
        );
    }

    #[test]
    fn tool_call_budget_fails_turn_before_appending_over_limit() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let transcript_status = TranscriptStatusState::new();
        input_handle.streaming_binding().set(true);
        let limits = AgentTurnLimits::new(8, 1, std::time::Duration::from_secs(30));
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime_with_limits(
            AgentConfig::defaults("."),
            sender,
            test_tool_registry(),
            test_tool_permissions(),
            limits,
        );
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        store.push(assistant);
        let branch = store.branch_token();
        let _cancel = mock_turns.start(assistant_id);

        assert!(apply_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
            AppAction::ToolCallsReady {
                branch,
                message_id: assistant_id,
                tool_calls: vec![
                    read_file_tool_call("call_1", "a.txt"),
                    read_file_tool_call("call_2", "b.txt"),
                ],
                mutating_tools_allowed: true,
                continue_after_tools: false,
            },
        ));

        let messages = store.messages();
        let ChatTurnStatus::Failed(error) = &messages[0].status else {
            panic!("expected failed turn, got {:?}", messages[0].status);
        };
        assert_eq!(error.kind, ChatErrorKind::Tool);
        assert!(error.message.contains("tool call limit"));
        assert_eq!(messages[0].blocks.len(), 1);
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(
            transcript_status
                .error_summary_state
                .get()
                .contains("err:tool")
        );
        assert!(!mock_turns.cancel(assistant_id));
    }

    #[test]
    fn tool_execution_timeout_writes_failed_result() {
        let mut registry = ToolRegistry::new();
        registry
            .register(SlowTool {
                delay: std::time::Duration::from_millis(80),
            })
            .expect("register slow tool");
        let limits = AgentTurnLimits::new(8, 16, std::time::Duration::from_millis(10));

        let result = execute_tool_use_to_result_block(
            &registry,
            &AgentConfig::defaults("."),
            &slow_tool_call("call_slow"),
            limits,
        );

        assert!(!result.ok);
        assert_eq!(result.call_id, "call_slow");
        match result.output {
            ToolOutput::Markdown(output) => {
                assert!(output.contains("Tool `slow_tool` timed out after 10ms"));
            }
            other => panic!("expected markdown timeout result, got {other:?}"),
        }
    }

    #[test]
    fn tool_calls_requiring_project_approval_render_approval_options() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();

        let block_id = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_run"),
        );

        let tool = tool_use_for_block(&store, block_id);
        assert_eq!(tool.status, ToolStatus::Pending);
        let approval = tool.approval.expect("run_command should require approval");
        let options = approval
            .options
            .iter()
            .map(|option| {
                (
                    option.id.as_str(),
                    option.label.as_str(),
                    option.action,
                    option.level,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(approval.id, "approval:call_run");
        assert!(approval.prompt.contains("run_command"));
        assert_eq!(
            options,
            vec![
                (
                    "allow_once",
                    "Allow once",
                    ApprovalAction::Allow,
                    ApprovalLevel::Once
                ),
                (
                    "allow_project",
                    "Allow project",
                    ApprovalAction::Allow,
                    ApprovalLevel::Project
                ),
                ("deny", "Deny", ApprovalAction::Deny, ApprovalLevel::Once),
            ]
        );
    }

    #[test]
    fn approval_allow_once_resolves_tool_without_project_grant() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            AgentConfig::defaults("."),
            sender,
            registry.clone(),
            permissions.clone(),
        );
        let block_id = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_once"),
        );
        let approval_id = tool_use_for_block(&store, block_id)
            .approval
            .expect("approval should exist")
            .id;

        handle_tool_approval(
            &store,
            &tool_runtime,
            approval_decision(
                &store,
                block_id,
                &approval_id,
                "allow_once",
                ApprovalAction::Allow,
                ApprovalLevel::Once,
            ),
        );

        let tool = tool_use_for_block(&store, block_id);
        assert_eq!(tool.status, ToolStatus::Running);
        assert_eq!(
            tool.approval.and_then(|approval| approval.resolved),
            Some(atto_ui_chat::ApprovalResolution {
                option_id: "allow_once".to_string(),
                action: ApprovalAction::Allow,
                level: ApprovalLevel::Once,
            })
        );
        assert!(
            !permissions
                .lock()
                .expect("tool permission policy lock poisoned")
                .is_project_allowed("run_command")
        );
    }

    #[test]
    fn approval_allow_project_records_grant_and_skips_future_approval() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            AgentConfig::defaults("."),
            sender,
            registry.clone(),
            permissions.clone(),
        );
        let first_block = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_project_1"),
        );
        let approval_id = tool_use_for_block(&store, first_block)
            .approval
            .expect("approval should exist")
            .id;

        handle_tool_approval(
            &store,
            &tool_runtime,
            approval_decision(
                &store,
                first_block,
                &approval_id,
                "allow_project",
                ApprovalAction::Allow,
                ApprovalLevel::Project,
            ),
        );

        assert!(
            permissions
                .lock()
                .expect("tool permission policy lock poisoned")
                .is_project_allowed("run_command")
        );
        let second_block = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_project_2"),
        );
        let second_tool = tool_use_for_block(&store, second_block);
        assert_eq!(second_tool.status, ToolStatus::Running);
        assert!(second_tool.approval.is_none());
    }

    #[test]
    fn plan_gate_blocks_mutating_tool_even_with_project_grant() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let transcript_status = TranscriptStatusState::new();
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        permissions
            .lock()
            .expect("tool permission policy lock poisoned")
            .allow_for_project("run_command");
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            AgentConfig::defaults("."),
            sender,
            registry.clone(),
            permissions.clone(),
        );
        let assistant_id = store.next_message_id();
        store.push(
            ChatMessage::text(assistant_id, ChatRole::Assistant, "")
                .with_status(ChatTurnStatus::Streaming),
        );
        let branch = store.branch_token();

        assert!(apply_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
            AppAction::ToolCallsReady {
                branch,
                message_id: assistant_id,
                tool_calls: vec![run_command_tool_call("call_plan_gate")],
                mutating_tools_allowed: false,
                continue_after_tools: false,
            },
        ));
        let block_id = store
            .messages()
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) if tool.call_id == "call_plan_gate" => Some(tool.id),
                _ => None,
            })
            .expect("blocked tool use should be appended");

        let tool = tool_use_for_block(&store, block_id);
        let result = tool_result_for_call(&store, "call_plan_gate");
        assert_eq!(tool.status, ToolStatus::Canceled);
        assert!(tool.approval.is_none());
        assert!(!result.ok);
        assert_eq!(result.exit_code, None);
        match result.output {
            ToolOutput::Markdown(output) => {
                assert_eq!(output, PLAN_MODE_MUTATING_TOOL_BLOCKED_RESULT);
            }
            other => panic!("expected markdown plan-gate result, got {other:?}"),
        }
    }

    #[test]
    fn approval_deny_cancels_tool_and_writes_failed_result() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        let (sender, _receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            AgentConfig::defaults("."),
            sender,
            registry.clone(),
            permissions.clone(),
        );
        let block_id = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_deny"),
        );
        let approval_id = tool_use_for_block(&store, block_id)
            .approval
            .expect("approval should exist")
            .id;

        handle_tool_approval(
            &store,
            &tool_runtime,
            approval_decision(
                &store,
                block_id,
                &approval_id,
                "deny",
                ApprovalAction::Deny,
                ApprovalLevel::Once,
            ),
        );

        let tool = tool_use_for_block(&store, block_id);
        let result = tool_result_for_call(&store, "call_deny");
        assert_eq!(tool.status, ToolStatus::Canceled);
        assert_eq!(
            tool.approval.and_then(|approval| approval.resolved),
            Some(atto_ui_chat::ApprovalResolution {
                option_id: "deny".to_string(),
                action: ApprovalAction::Deny,
                level: ApprovalLevel::Once,
            })
        );
        assert!(!result.ok);
        assert_eq!(result.exit_code, None);
        match result.output {
            ToolOutput::Markdown(output) => {
                assert!(output.contains("User denied tool call `run_command`"));
            }
            other => panic!("expected markdown denial result, got {other:?}"),
        }
    }

    #[test]
    fn allowed_tool_execution_writes_tool_result_block() {
        let workspace = test_workspace("allowed-tool-result");
        fs::write(workspace.join("fixture.txt"), "tool output\n").expect("write fixture file");
        let mut config = AgentConfig::defaults(workspace.clone());
        config.workspace = workspace.clone();
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let transcript_status = TranscriptStatusState::new();
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();
        let (sender, receiver) = atto_ui::reactive::EventQueue::<AppAction>::channel();
        let tool_runtime = test_tool_runtime(
            config.clone(),
            sender,
            registry.clone(),
            permissions.clone(),
        );
        let assistant_id = store.next_message_id();
        store.push(
            ChatMessage::text(assistant_id, ChatRole::Assistant, "")
                .with_status(ChatTurnStatus::Streaming),
        );
        let branch = store.branch_token();

        assert!(apply_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
            AppAction::ToolCallsReady {
                branch,
                message_id: assistant_id,
                tool_calls: vec![read_file_tool_call("call_read", "fixture.txt")],
                mutating_tools_allowed: true,
                continue_after_tools: false,
            },
        ));
        let tool_block_id = store
            .messages()
            .iter()
            .flat_map(|message| message.blocks.iter())
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) if tool.call_id == "call_read" => Some(tool.id),
                _ => None,
            })
            .expect("tool use should be appended");
        assert_eq!(
            tool_use_for_block(&store, tool_block_id).status,
            ToolStatus::Running
        );

        let action = receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("tool execution should send result action");
        assert!(apply_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &transcript_status,
            &tool_runtime,
            action,
        ));

        let tool = tool_use_for_block(&store, tool_block_id);
        let result = tool_result_for_call(&store, "call_read");
        assert_eq!(tool.status, ToolStatus::Done);
        assert!(result.ok);
        assert_eq!(result.exit_code, None);
        match result.output {
            ToolOutput::Markdown(output) => {
                assert!(output.contains("Path: `fixture.txt`"));
                assert!(output.contains("tool output"));
            }
            other => panic!("expected markdown tool result, got {other:?}"),
        }

        fs::remove_dir_all(&workspace).expect("remove fixture workspace");
    }

    #[test]
    fn unknown_tool_call_writes_failed_tool_result_without_execution() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let registry = test_tool_registry();
        let permissions = test_tool_permissions();

        let block_id = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            unknown_tool_call("call_missing"),
        );

        let tool = tool_use_for_block(&store, block_id);
        let result = tool_result_for_call(&store, "call_missing");
        assert_eq!(tool.status, ToolStatus::Error);
        assert!(tool.approval.is_none());
        assert!(!result.ok);
        match result.output {
            ToolOutput::Markdown(output) => {
                assert!(output.contains("Tool `missing_tool` is not registered."));
            }
            other => panic!("expected markdown missing-tool result, got {other:?}"),
        }
    }

    #[test]
    fn deepseek_request_from_transcript_includes_tool_result_role_message() {
        let registry = test_tool_registry();
        let request = deepseek_request_from_transcript(
            &AgentConfig::defaults("."),
            &registry,
            &[
                ChatMessage::text(1, ChatRole::User, "Read the fixture."),
                ChatMessage::new(
                    2,
                    ChatRole::Assistant,
                    vec![
                        ChatBlock::ToolUse(read_file_tool_call("call_read", "fixture.txt")),
                        ChatBlock::ToolResult(ToolResultBlock {
                            id: atto_ui_chat::ChatBlockId::new(22),
                            call_id: "call_read".to_string(),
                            ok: true,
                            exit_code: None,
                            output: ToolOutput::Markdown("Path: `fixture.txt`\n\nbody".to_string()),
                            collapsed: false,
                        }),
                    ],
                ),
            ],
        );

        assert_eq!(request.messages.len(), 3);
        assert_eq!(request.messages[0].role, ChatMessageRole::User);
        assert_eq!(
            request.messages[0].content.as_deref(),
            Some("Read the fixture.")
        );
        assert_eq!(request.messages[1].role, ChatMessageRole::Assistant);
        assert_eq!(request.messages[1].tool_calls.len(), 1);
        let tool_call = &request.messages[1].tool_calls[0];
        assert_eq!(tool_call.id, "call_read");
        assert_eq!(tool_call.function.name, "read_file");
        assert_eq!(tool_call.function.arguments, r#"{"path":"fixture.txt"}"#);
        assert_eq!(request.messages[2].role, ChatMessageRole::Tool);
        assert_eq!(
            request.messages[2].tool_call_id.as_deref(),
            Some("call_read")
        );
        assert!(
            request.messages[2]
                .content
                .as_deref()
                .is_some_and(|content| content.contains("ok: true") && content.contains("body"))
        );
        assert_eq!(request.tools.len(), registry.len());
        assert_eq!(
            request.tool_choice,
            Some(ToolChoice::Mode(ToolChoiceMode::Auto))
        );
    }

    #[test]
    fn deepseek_request_from_transcript_injects_file_mentions_from_config_workspace() {
        let workspace = unique_temp_dir("request-file-mentions");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::write(workspace.join("note.txt"), "workspace context\n").expect("write fixture");
        let config = AgentConfig::defaults(workspace.clone());
        let registry = test_tool_registry();

        let request = deepseek_request_from_transcript(
            &config,
            &registry,
            &[ChatMessage::text(1, ChatRole::User, "Use @note.txt")],
        );

        let content = request.messages[0]
            .content
            .as_deref()
            .expect("user message should contain text");
        assert!(content.contains("<context_files>"));
        assert!(content.contains("<file path=\"note.txt\""));
        assert!(content.contains("workspace context"));

        fs::remove_dir_all(workspace).expect("remove fixture workspace");
    }

    #[test]
    fn deepseek_plan_request_forces_submit_plan_virtual_tool() {
        let request = deepseek_plan_request_from_transcript(
            &AgentConfig::defaults("."),
            &[ChatMessage::text(
                1,
                ChatRole::User,
                "Please update README and run tests.",
            )],
        );

        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, ChatMessageRole::System);
        assert!(
            request.messages[0]
                .content
                .as_deref()
                .is_some_and(|content| content.contains("You are in plan mode"))
        );
        assert_eq!(request.messages[1].role, ChatMessageRole::User);
        assert_eq!(request.tools.len(), 1);
        assert_eq!(
            request.tools[0].function.name,
            crate::plan::SUBMIT_PLAN_TOOL_NAME
        );
        assert_eq!(
            request.tool_choice,
            Some(ToolChoice::Function(
                crate::deepseek::ToolChoiceFunction::named(crate::plan::SUBMIT_PLAN_TOOL_NAME,)
            ))
        );
    }

    #[test]
    fn deepseek_stream_submit_plan_tool_call_writes_pending_plan_block() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let text_block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();
        let _cancel = mock_turns.start(assistant_id);
        let mut stream = DeepSeekUiStream::new_with_plan_requirement(
            branch,
            assistant_id,
            text_block_id,
            "deepseek-chat",
            true,
        );
        let events = vec![
            ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
                id: None,
                object: None,
                created: None,
                model: None,
                choices: vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: ChatCompletionDelta {
                        tool_calls: vec![tool_call_delta(
                            0,
                            Some("call_plan"),
                            Some(crate::plan::SUBMIT_PLAN_TOOL_NAME),
                            Some(
                                r#"{"items":["Inspect the current implementation.","Add submit_plan mapping.","Run validation."]}"#,
                            ),
                        )],
                        ..ChatCompletionDelta::default()
                    },
                    finish_reason: Some(crate::deepseek::FinishReason::ToolCalls),
                }],
                usage: None,
            }),
            ChatCompletionSseEvent::Done,
        ];

        for event in events {
            for action in stream.map_event(event) {
                assert!(apply_test_app_action(
                    &store,
                    &input_handle,
                    &mock_turns,
                    &status_state,
                    action,
                ));
            }
        }

        let messages = store.messages();
        let assistant = &messages[0];
        assert_eq!(assistant.status, ChatTurnStatus::Complete);
        assert_eq!(assistant.meta.stop_reason, Some(StopReason::EndTurn));
        assert_eq!(message_text(assistant), "");
        match &assistant.blocks[1] {
            ChatBlock::Plan(plan) => {
                assert_eq!(plan.decision, PlanDecision::Pending);
                assert_eq!(
                    plan.items
                        .iter()
                        .map(|item| item.text.as_str())
                        .collect::<Vec<_>>(),
                    vec![
                        "Inspect the current implementation.",
                        "Add submit_plan mapping.",
                        "Run validation."
                    ]
                );
            }
            other => panic!("expected plan block, got {other:?}"),
        }
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(!mock_turns.cancel(assistant_id));
    }

    #[test]
    fn deepseek_stream_plan_turn_mutating_tool_call_writes_blocked_tool_result() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let text_block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();
        let _cancel = mock_turns.start(assistant_id);
        let mut stream = DeepSeekUiStream::new_with_plan_requirement(
            branch,
            assistant_id,
            text_block_id,
            "deepseek-chat",
            true,
        );
        let events = vec![
            ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
                id: None,
                object: None,
                created: None,
                model: None,
                choices: vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: ChatCompletionDelta {
                        tool_calls: vec![tool_call_delta(
                            0,
                            Some("call_blocked_run"),
                            Some("run_command"),
                            Some(r#"{"argv":["/bin/echo","blocked"],"cwd":"."}"#),
                        )],
                        ..ChatCompletionDelta::default()
                    },
                    finish_reason: Some(crate::deepseek::FinishReason::ToolCalls),
                }],
                usage: None,
            }),
            ChatCompletionSseEvent::Done,
        ];

        for event in events {
            for action in stream.map_event(event) {
                assert!(apply_test_app_action(
                    &store,
                    &input_handle,
                    &mock_turns,
                    &status_state,
                    action,
                ));
            }
        }

        let messages = store.messages();
        let assistant = &messages[0];
        assert_eq!(assistant.status, ChatTurnStatus::Complete);
        let tool = assistant
            .blocks
            .iter()
            .find_map(|block| match block {
                ChatBlock::ToolUse(tool) => Some(tool),
                _ => None,
            })
            .expect("blocked tool use should be appended");
        assert_eq!(tool.call_id, "call_blocked_run");
        assert_eq!(tool.name, "run_command");
        assert_eq!(tool.status, ToolStatus::Canceled);
        assert!(tool.approval.is_none());
        let result = tool_result_for_call(&store, "call_blocked_run");
        assert!(!result.ok);
        match result.output {
            ToolOutput::Markdown(output) => {
                assert_eq!(output, PLAN_MODE_MUTATING_TOOL_BLOCKED_RESULT);
            }
            other => panic!("expected markdown plan-gate result, got {other:?}"),
        }
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn deepseek_stream_markdown_plan_fallback_writes_pending_plan_block() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let text_block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();
        let _cancel = mock_turns.start(assistant_id);
        let mut stream = DeepSeekUiStream::new_with_plan_requirement(
            branch,
            assistant_id,
            text_block_id,
            "deepseek-chat",
            true,
        );
        let events = parse_chat_completion_sse(concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Plan:\\n1. Inspect current state.\\n\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"2. Implement the plan parser.\\n- [ ] Run validation.\\n\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: [DONE]\n\n",
        ))
        .unwrap();

        for event in events {
            for action in stream.map_event(event) {
                assert!(apply_test_app_action(
                    &store,
                    &input_handle,
                    &mock_turns,
                    &status_state,
                    action,
                ));
            }
        }

        let messages = store.messages();
        let assistant = &messages[0];
        assert_eq!(assistant.status, ChatTurnStatus::Complete);
        assert_eq!(message_text(assistant), "");
        match &assistant.blocks[1] {
            ChatBlock::Plan(plan) => {
                assert_eq!(plan.decision, PlanDecision::Pending);
                assert_eq!(
                    plan.items
                        .iter()
                        .map(|item| item.text.as_str())
                        .collect::<Vec<_>>(),
                    vec![
                        "Inspect current state.",
                        "Implement the plan parser.",
                        "Run validation."
                    ]
                );
            }
            other => panic!("expected plan block, got {other:?}"),
        }
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn deepseek_request_from_transcript_injects_loaded_skills() {
        let (workspace, skill_registry) = test_skill_registry(&[
            ("rust", "rust-review", "Review Rust code."),
            ("docs", "docs", "Write documentation."),
        ]);
        let loaded = LoadedSkillSet::default();
        assert!(loaded.insert("rust-review"));
        let registry = test_tool_registry();

        let request = deepseek_request_from_transcript_with_skills(
            &AgentConfig::defaults("."),
            &registry,
            &skill_registry,
            &loaded,
            &[ChatMessage::text(1, ChatRole::User, "Please review this.")],
        );

        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, ChatMessageRole::System);
        let skill_prompt = request.messages[0]
            .content
            .as_deref()
            .expect("skill system prompt should have content");
        assert!(skill_prompt.starts_with("<skills>\n"));
        assert!(skill_prompt.contains("<skill name=\"rust-review\" source=\""));
        assert!(skill_prompt.contains("Use this skill for rust-review tasks."));
        assert!(!skill_prompt.contains("Use this skill for docs tasks."));
        assert!(skill_prompt.ends_with("</skills>"));
        assert_eq!(request.messages[1].role, ChatMessageRole::User);
        assert_eq!(request.tools.len(), registry.len());

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn skill_tool_preferences_do_not_grant_mutating_tool_approval() {
        let workspace = unique_temp_dir("skill-tool-permissions");
        write_test_skill_with_mode_and_tools(
            &workspace,
            "shell",
            "shell-helper",
            "Prefer shell diagnostics.",
            SkillMode::Manual,
            &[],
            &["run_command"],
        );
        let skill_registry =
            SkillRegistry::discover_from_paths(&[SkillSearchPath::workspace(&workspace)]);
        let loaded = LoadedSkillSet::default();
        assert!(loaded.insert("shell-helper"));
        let registry = test_tool_registry();

        let request = deepseek_request_from_transcript_with_skills(
            &AgentConfig::defaults("."),
            &registry,
            &skill_registry,
            &loaded,
            &[ChatMessage::text(1, ChatRole::User, "Run diagnostics.")],
        );

        let skill_prompt = request.messages[0]
            .content
            .as_deref()
            .expect("skill prompt should be injected");
        assert!(skill_prompt.contains("tools=\"run_command\""));
        assert_eq!(request.tools.len(), registry.len());
        assert_eq!(
            registry
                .spec("run_command")
                .expect("run_command tool should be registered")
                .permission,
            ToolPermission::ApproveForProject
        );

        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        let permissions = test_tool_permissions();
        let block_id = append_tool_call_with_runtime(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            &registry,
            &permissions,
            run_command_tool_call("call_skill_run"),
        );

        let tool = tool_use_for_block(&store, block_id);
        assert_eq!(tool.status, ToolStatus::Pending);
        assert!(tool.approval.is_some());
        assert!(
            !permissions
                .lock()
                .expect("tool permission policy lock poisoned")
                .is_project_allowed("run_command")
        );

        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn deepseek_error_mapping_covers_http_network_disconnect_and_json_failures() {
        let mut stream = new_test_stream();
        let error = single_failed_error(stream.map_error(chat_error_from_http_status(
            401,
            r#"{"error":{"message":"bad api key","type":"invalid_request_error","code":"invalid_api_key","param":null}}"#,
        )));
        assert_eq!(error.kind, ChatErrorKind::Api);
        assert!(error.message.contains("DEEPSEEK_API_KEY"));
        let detail = error.detail.as_deref().expect("detail should be present");
        assert!(detail.contains("HTTP status: 401"));
        assert!(detail.contains("invalid_api_key"));

        let mut stream = new_test_stream();
        let error = single_failed_error(
            stream.map_error(chat_error_from_http_status(429, "rate limit body")),
        );
        assert_eq!(error.kind, ChatErrorKind::RateLimit);
        assert!(error.message.contains("429"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("rate limit body"))
        );

        let mut stream = new_test_stream();
        let error =
            single_failed_error(stream.map_error(chat_error_from_http_status(502, "gateway down")));
        assert_eq!(error.kind, ChatErrorKind::Api);
        assert!(error.message.contains("502"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("gateway down"))
        );

        let mut stream = new_test_stream();
        let error = single_failed_error(
            stream.map_error(chat_error_from_network_failure("request timed out")),
        );
        assert_eq!(error.kind, ChatErrorKind::Network);
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("request timed out"))
        );

        let mut stream = new_test_stream();
        let error = single_failed_error(stream.map_error(chat_error_from_stream_disconnect()));
        assert_eq!(error.kind, ChatErrorKind::Network);
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("[DONE]"))
        );

        let json_error = parse_chat_completion_sse_data("{not json").unwrap_err();
        let mut stream = new_test_stream();
        let error = single_failed_error(
            stream.map_error(chat_error_from_json_error(json_error, "{not json")),
        );
        assert_eq!(error.kind, ChatErrorKind::Api);
        assert!(error.message.contains("parse DeepSeek stream JSON"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("{not json"))
        );
    }

    #[test]
    fn deepseek_stream_error_event_fails_turn_with_structured_detail() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let text_block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();
        let _cancel = mock_turns.start(assistant_id);
        let mut stream =
            DeepSeekUiStream::new(branch, assistant_id, text_block_id, "deepseek-chat");
        let events = parse_chat_completion_sse(
            "data: {\"error\":{\"message\":\"bad api key\",\"type\":\"invalid_request_error\",\"code\":\"invalid_api_key\",\"param\":null}}\n\n",
        )
        .unwrap();

        for event in events {
            for action in stream.map_event(event) {
                assert!(apply_test_app_action(
                    &store,
                    &input_handle,
                    &mock_turns,
                    &status_state,
                    action,
                ));
            }
        }

        let messages = store.messages();
        let ChatTurnStatus::Failed(error) = &messages[0].status else {
            panic!("expected failed turn, got {:?}", messages[0].status);
        };
        assert_eq!(error.kind, ChatErrorKind::Api);
        assert!(error.message.contains("DEEPSEEK_API_KEY"));
        let detail = error.detail.as_deref().expect("detail should be present");
        assert!(detail.contains("bad api key"));
        assert!(detail.contains("invalid_request_error"));
        assert!(detail.contains("invalid_api_key"));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
        assert!(!mock_turns.cancel(assistant_id));
        assert!(!apply_test_app_action(
            &store,
            &input_handle,
            &mock_turns,
            &status_state,
            AppAction::TextDelta {
                branch,
                block_id: text_block_id,
                delta: "late".to_string(),
            },
        ));
        assert_eq!(message_text(&store.messages()[0]), "");
    }

    #[test]
    fn deepseek_stream_events_aggregate_tool_calls_by_index() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let text_block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();
        let _cancel = mock_turns.start(assistant_id);
        let mut stream =
            DeepSeekUiStream::new(branch, assistant_id, text_block_id, "deepseek-chat");
        let events = vec![
            ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
                id: None,
                object: None,
                created: None,
                model: Some("deepseek-chat".to_string()),
                choices: vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: ChatCompletionDelta {
                        tool_calls: vec![
                            tool_call_delta(
                                1,
                                Some("call_2"),
                                Some("search_text"),
                                Some(r#"{"query":"hel"#),
                            ),
                            tool_call_delta(0, Some("call_1"), Some("read_"), Some(r#"{"path":"#)),
                        ],
                        ..ChatCompletionDelta::default()
                    },
                    finish_reason: None,
                }],
                usage: None,
            }),
            ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
                id: None,
                object: None,
                created: None,
                model: None,
                choices: vec![ChatCompletionChunkChoice {
                    index: 0,
                    delta: ChatCompletionDelta {
                        tool_calls: vec![
                            tool_call_delta(0, None, Some("file"), Some(r#""src/lib.rs"}"#)),
                            tool_call_delta(1, None, None, Some(r#"lo"}"#)),
                        ],
                        ..ChatCompletionDelta::default()
                    },
                    finish_reason: Some(crate::deepseek::FinishReason::ToolCalls),
                }],
                usage: None,
            }),
            ChatCompletionSseEvent::Done,
        ];

        for event in events {
            for action in stream.map_event(event) {
                assert!(apply_test_app_action(
                    &store,
                    &input_handle,
                    &mock_turns,
                    &status_state,
                    action,
                ));
            }
        }

        let messages = store.messages();
        let assistant = &messages[0];
        assert_eq!(assistant.status, ChatTurnStatus::Complete);
        assert_eq!(assistant.meta.stop_reason, Some(StopReason::ToolUse));
        assert_eq!(assistant.blocks.len(), 3);
        assert!(
            matches!(&assistant.blocks[0], ChatBlock::Text(block) if block.markdown.is_empty() && !block.streaming)
        );
        match &assistant.blocks[1] {
            ChatBlock::ToolUse(block) => {
                assert_eq!(block.call_id, "call_1");
                assert_eq!(block.name, "read_file");
                assert_eq!(block.status, ToolStatus::Running);
                assert!(block.approval.is_none());
                match &block.input {
                    ToolInput::Json(ComponentValue::Map(input)) => assert_eq!(
                        input.get("path"),
                        Some(&ComponentValue::String("src/lib.rs".to_string()))
                    ),
                    other => panic!("expected JSON object tool input, got {other:?}"),
                }
            }
            other => panic!("expected first tool call block, got {other:?}"),
        }
        match &assistant.blocks[2] {
            ChatBlock::ToolUse(block) => {
                assert_eq!(block.call_id, "call_2");
                assert_eq!(block.name, "search_text");
                assert_eq!(block.status, ToolStatus::Running);
                match &block.input {
                    ToolInput::Json(ComponentValue::Map(input)) => assert_eq!(
                        input.get("query"),
                        Some(&ComponentValue::String("hello".to_string()))
                    ),
                    other => panic!("expected JSON object tool input, got {other:?}"),
                }
            }
            other => panic!("expected second tool call block, got {other:?}"),
        }
        assert!(!mock_turns.cancel(assistant_id));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }

    #[test]
    fn deepseek_stream_tool_call_invalid_arguments_fails_turn() {
        let mut stream = new_test_stream();
        let actions = stream.map_event(ChatCompletionSseEvent::Chunk(ChatCompletionChunk {
            id: None,
            object: None,
            created: None,
            model: None,
            choices: vec![ChatCompletionChunkChoice {
                index: 0,
                delta: ChatCompletionDelta {
                    tool_calls: vec![tool_call_delta(
                        0,
                        Some("call_1"),
                        Some("read_file"),
                        Some("{not json"),
                    )],
                    ..ChatCompletionDelta::default()
                },
                finish_reason: Some(crate::deepseek::FinishReason::ToolCalls),
            }],
            usage: None,
        }));

        let error = single_failed_error(actions);

        assert_eq!(error.kind, ChatErrorKind::Tool);
        assert!(error.message.contains("invalid tool call arguments"));
        assert!(
            error
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("{not json"))
        );
        assert!(stream.map_event(ChatCompletionSseEvent::Done).is_empty());
    }

    #[test]
    fn deepseek_stream_events_map_reasoning_content_and_completion_meta() {
        let store = ChatMessageStore::new();
        let input_handle = atto_ui_chat::ChatInputHandle::new();
        let mock_turns = MockTurnRegistry::new();
        let status_state = atto_ui::reactive::Property::new(STATUS_STREAMING.to_string());
        input_handle.streaming_binding().set(true);
        let assistant_id = store.next_message_id();
        let assistant = ChatMessage::text(assistant_id, ChatRole::Assistant, "")
            .with_status(ChatTurnStatus::Streaming);
        let text_block_id = assistant.blocks[0].id();
        store.push(assistant);
        let branch = store.branch_token();
        let _cancel = mock_turns.start(assistant_id);
        let mut stream =
            DeepSeekUiStream::new(branch, assistant_id, text_block_id, "deepseek-chat");
        let events = parse_chat_completion_sse(concat!(
            "data: {\"model\":\"deepseek-reasoner\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"think \"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"more\",\"content\":\"hel\"},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":5,\"total_tokens\":12}}\n\n",
            "data: [DONE]\n\n",
        ))
        .unwrap();

        for event in events {
            for action in stream.map_event(event) {
                assert!(apply_test_app_action(
                    &store,
                    &input_handle,
                    &mock_turns,
                    &status_state,
                    action,
                ));
            }
        }

        let messages = store.messages();
        let assistant = &messages[0];
        assert_eq!(assistant.status, ChatTurnStatus::Complete);
        assert_eq!(assistant.meta.model.as_deref(), Some("deepseek-reasoner"));
        assert_eq!(
            assistant.meta.usage,
            Some(TokenUsage {
                input: 7,
                output: 5,
            })
        );
        assert_eq!(assistant.meta.stop_reason, Some(StopReason::EndTurn));
        assert_eq!(assistant.blocks.len(), 2);
        match &assistant.blocks[0] {
            ChatBlock::Thinking(block) => {
                assert_eq!(block.markdown, "think more");
                assert!(block.collapsed);
                assert!(!block.streaming);
            }
            other => panic!("expected thinking block, got {other:?}"),
        }
        match &assistant.blocks[1] {
            ChatBlock::Text(block) => {
                assert_eq!(block.markdown, "hello");
                assert!(!block.streaming);
            }
            other => panic!("expected text block, got {other:?}"),
        }
        assert!(!mock_turns.cancel(assistant_id));
        assert!(!input_handle.streaming_binding().get());
        assert_eq!(status_state.get(), STATUS_READY);
    }
