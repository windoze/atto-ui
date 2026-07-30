//! Runtime state and bootstrap: request DTOs, the agent/tool/slash/mock runtimes,
//! transcript persistence, and the top-level run-loop wiring helpers.

use crate::*;

#[derive(Clone, Debug)]
pub(crate) struct MockAgentTurnRequest {
    pub(crate) branch: ChatBranchToken,
    pub(crate) message_id: ChatMessageId,
    pub(crate) block_id: ChatBlockId,
    pub(crate) cancel: CancellationToken,
    pub(crate) token_delay: Duration,
    pub(crate) model: String,
    pub(crate) prompt: String,
    pub(crate) plan_decision: PlanTurnDecision,
    pub(crate) mutating_tools_allowed: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct DeepSeekAgentTurnRequest {
    pub(crate) branch: ChatBranchToken,
    pub(crate) message_id: ChatMessageId,
    pub(crate) block_id: ChatBlockId,
    pub(crate) cancel: CancellationToken,
    pub(crate) config: AgentConfig,
    pub(crate) request: ChatCompletionRequest,
    pub(crate) plan_decision: PlanTurnDecision,
    pub(crate) mutating_tools_allowed: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct AgentTurnStartRequest {
    pub(crate) prompt: String,
    pub(crate) plan_decision: PlanTurnDecision,
    pub(crate) mutating_tools_allowed: bool,
    pub(crate) skill_registry: SkillRegistry,
    pub(crate) loaded_skills: LoadedSkillSet,
}

#[derive(Clone)]
pub(crate) struct ToolExecutionRequest {
    pub(crate) branch: ChatBranchToken,
    pub(crate) message_id: ChatMessageId,
    pub(crate) tool_use: ToolUseBlock,
    pub(crate) config: AgentConfig,
    pub(crate) registry: ToolRegistry,
    pub(crate) limits: AgentTurnLimits,
    pub(crate) action_sender: mpsc::Sender<AppAction>,
    pub(crate) mutating_tools_allowed: bool,
    pub(crate) continue_after_tools: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct TranscriptStatusState {
    pub(crate) token_estimate_state: Property<String>,
    pub(crate) error_summary_state: Property<String>,
}

pub(crate) struct StatusSegmentBindings {
    pub(crate) model: Binding<String>,
    pub(crate) provider: Binding<String>,
    pub(crate) state: Binding<String>,
    pub(crate) plan_mode: Binding<String>,
    pub(crate) tools: Binding<String>,
    pub(crate) skills: Binding<String>,
    pub(crate) tokens: Binding<String>,
    pub(crate) error: Binding<String>,
}

impl TranscriptStatusState {
    pub(crate) fn new() -> Self {
        Self {
            token_estimate_state: Property::new(format_token_estimate_status(0)),
            error_summary_state: Property::new(error_summary_status(&[])),
        }
    }

    pub(crate) fn sync(&self, store: &ChatMessageStore) {
        sync_transcript_status(store, &self.token_estimate_state, &self.error_summary_state);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SlashRuntime {
    pub(crate) input_handle: ChatInputHandle,
    pub(crate) mock_turns: MockTurnRegistry,
    pub(crate) status_state: Property<String>,
    pub(crate) plan_mode_state: Property<String>,
    pub(crate) skill_registry: SkillRegistry,
    pub(crate) loaded_skills: LoadedSkillSet,
    pub(crate) skill_count_state: Property<String>,
    pub(crate) transcript_status: TranscriptStatusState,
    pub(crate) turn_budgets: TurnBudgetTracker,
}

#[derive(Clone)]
pub(crate) struct AgentTurnLauncher {
    pub(crate) config: AgentConfig,
    pub(crate) action_sender: mpsc::Sender<AppAction>,
    pub(crate) tool_registry: ToolRegistry,
    pub(crate) turn_budgets: TurnBudgetTracker,
    pub(crate) limits: AgentTurnLimits,
    pub(crate) compact_policy: CompactPolicy,
}

#[derive(Clone)]
pub(crate) struct PlanDecisionRuntime {
    pub(crate) input_handle: ChatInputHandle,
    pub(crate) mock_turns: MockTurnRegistry,
    pub(crate) status_state: Property<String>,
    pub(crate) skill_registry: SkillRegistry,
    pub(crate) loaded_skills: LoadedSkillSet,
    pub(crate) transcript_status: TranscriptStatusState,
    pub(crate) turn_launcher: AgentTurnLauncher,
}

#[derive(Clone, Debug)]
pub(crate) struct MockTurnRegistry {
    pub(crate) current: Arc<Mutex<Option<ActiveMockTurn>>>,
    pub(crate) token_delay: Duration,
}

#[derive(Clone, Debug)]
pub(crate) struct ActiveMockTurn {
    pub(crate) message_id: ChatMessageId,
    pub(crate) cancel: CancellationToken,
    pub(crate) abort_handle: Option<AbortHandle>,
}

#[derive(Clone)]
pub(crate) struct AgentRuntime {
    pub(crate) config: AgentConfig,
    pub(crate) action_sender: mpsc::Sender<AppAction>,
    pub(crate) mock_turns: MockTurnRegistry,
    pub(crate) skill_registry: SkillRegistry,
    pub(crate) loaded_skills: LoadedSkillSet,
    pub(crate) tool_registry: ToolRegistry,
    pub(crate) tool_permissions: Arc<Mutex<ToolPermissionPolicy>>,
    pub(crate) turn_budgets: TurnBudgetTracker,
    pub(crate) limits: AgentTurnLimits,
    pub(crate) message_store: ChatMessageStore,
    pub(crate) input_handle: ChatInputHandle,
    pub(crate) status_state: Property<String>,
    pub(crate) provider_state: Property<String>,
    pub(crate) model_state: Property<String>,
    pub(crate) plan_mode_state: Property<String>,
    pub(crate) tool_count_state: Property<String>,
    pub(crate) skill_count_state: Property<String>,
    pub(crate) transcript_status: TranscriptStatusState,
    /// Right-click context menus surfaced by the chat list, drained in the run
    /// loop where a `Desktop` is available to spawn the popup window.
    pub(crate) context_menus: EventQueue<ChatContextMenuRequest>,
}

#[derive(Clone)]
pub(crate) struct ToolRuntime {
    pub(crate) config: AgentConfig,
    pub(crate) action_sender: mpsc::Sender<AppAction>,
    pub(crate) registry: ToolRegistry,
    pub(crate) permissions: Arc<Mutex<ToolPermissionPolicy>>,
    pub(crate) turn_budgets: TurnBudgetTracker,
    pub(crate) limits: AgentTurnLimits,
    pub(crate) input_handle: ChatInputHandle,
    pub(crate) mock_turns: MockTurnRegistry,
    pub(crate) status_state: Property<String>,
    pub(crate) skill_registry: SkillRegistry,
    pub(crate) loaded_skills: LoadedSkillSet,
    pub(crate) transcript_status: TranscriptStatusState,
}

pub(crate) struct TranscriptPersistence {
    pub(crate) path: Option<PathBuf>,
    pub(crate) messages: Binding<Vec<ChatMessage>>,
    pub(crate) observer: DirtyObserver,
    pub(crate) pending_dirty: bool,
    pub(crate) last_save: Option<Instant>,
}

impl TranscriptPersistence {
    pub(crate) fn new(path: Option<PathBuf>, store: &ChatMessageStore) -> Self {
        let messages = store.binding();
        let observer = messages.dirty_observer();
        Self {
            path,
            messages,
            observer,
            pending_dirty: false,
            last_save: None,
        }
    }

    pub(crate) fn save_if_dirty(&mut self) -> Result<()> {
        let Some(path) = self.path.as_deref() else {
            return Ok(());
        };
        if self.messages.check_dirty(&mut self.observer) {
            self.pending_dirty = true;
        }
        if !self.pending_dirty {
            return Ok(());
        }
        if self
            .last_save
            .is_some_and(|last_save| last_save.elapsed() < TRANSCRIPT_SAVE_DEBOUNCE)
        {
            return Ok(());
        }
        save_transcript_jsonl(path, &self.messages.get())?;
        self.pending_dirty = false;
        self.last_save = Some(Instant::now());
        Ok(())
    }

    pub(crate) fn save_now(&mut self) -> Result<()> {
        let Some(path) = self.path.as_deref() else {
            return Ok(());
        };
        save_transcript_jsonl(path, &self.messages.get())?;
        self.pending_dirty = false;
        self.last_save = Some(Instant::now());
        Ok(())
    }
}

impl AgentRuntime {
    pub(crate) fn new(
        config: AgentConfig,
        action_sender: mpsc::Sender<AppAction>,
        mock_turns: MockTurnRegistry,
    ) -> Self {
        let model_state = Property::new(format!("model: {}", config.model));
        let provider_state = Property::new(config.provider.status());
        let plan_mode_state = Property::new(config.plan_mode.status());
        let tool_registry =
            crate::tool::builtin_tool_registry().expect("built-in tool registry must be valid");
        let tool_count_state = Property::new(format_tool_count_status(tool_registry.len()));
        let skill_registry = SkillRegistry::discover(&config.workspace, config.home_dir.as_deref());
        let loaded_skills = LoadedSkillSet::default();
        let limits = AgentTurnLimits::default();
        let turn_budgets = TurnBudgetTracker::default();
        let skill_count_state = Property::new(loaded_skills.status());
        let transcript_status = TranscriptStatusState::new();
        Self {
            config,
            action_sender,
            mock_turns,
            skill_registry,
            loaded_skills,
            tool_registry,
            tool_permissions: Arc::new(Mutex::new(ToolPermissionPolicy::default())),
            turn_budgets,
            limits,
            message_store: ChatMessageStore::new(),
            input_handle: ChatInputHandle::new(),
            status_state: Property::new(STATUS_READY.to_string()),
            provider_state,
            model_state,
            plan_mode_state,
            tool_count_state,
            skill_count_state,
            transcript_status,
            context_menus: EventQueue::new(),
        }
    }

    pub(crate) fn tool_runtime(&self) -> ToolRuntime {
        ToolRuntime {
            config: self.config.clone(),
            action_sender: self.action_sender.clone(),
            registry: self.tool_registry.clone(),
            permissions: self.tool_permissions.clone(),
            turn_budgets: self.turn_budgets.clone(),
            limits: self.limits,
            input_handle: self.input_handle.clone(),
            mock_turns: self.mock_turns.clone(),
            status_state: self.status_state.clone(),
            skill_registry: self.skill_registry.clone(),
            loaded_skills: self.loaded_skills.clone(),
            transcript_status: self.transcript_status.clone(),
        }
    }

    pub(crate) fn slash_runtime(&self) -> SlashRuntime {
        SlashRuntime {
            input_handle: self.input_handle.clone(),
            mock_turns: self.mock_turns.clone(),
            status_state: self.status_state.clone(),
            plan_mode_state: self.plan_mode_state.clone(),
            skill_registry: self.skill_registry.clone(),
            loaded_skills: self.loaded_skills.clone(),
            skill_count_state: self.skill_count_state.clone(),
            transcript_status: self.transcript_status.clone(),
            turn_budgets: self.turn_budgets.clone(),
        }
    }
}

impl MockTurnRegistry {
    pub(crate) fn new() -> Self {
        Self::with_token_delay(MOCK_TOKEN_DELAY)
    }

    pub(crate) fn with_token_delay(token_delay: Duration) -> Self {
        Self {
            current: Arc::new(Mutex::new(None)),
            token_delay,
        }
    }

    pub(crate) fn token_delay(&self) -> Duration {
        self.token_delay
    }

    pub(crate) fn start(&self, message_id: ChatMessageId) -> CancellationToken {
        self.start_with_abort_handle(message_id, None)
    }

    pub(crate) fn start_with_abort_handle(
        &self,
        message_id: ChatMessageId,
        abort_handle: Option<AbortHandle>,
    ) -> CancellationToken {
        let cancel = CancellationToken::new();
        *self.current.lock().expect("active turn lock poisoned") = Some(ActiveMockTurn {
            message_id,
            cancel: cancel.clone(),
            abort_handle,
        });
        cancel
    }

    pub(crate) fn cancel(&self, message_id: ChatMessageId) -> bool {
        let mut current = self.current.lock().expect("active turn lock poisoned");
        if !current
            .as_ref()
            .is_some_and(|turn| turn.message_id == message_id)
        {
            return false;
        }
        let Some(turn) = current.take() else {
            return false;
        };
        Self::cancel_active_turn(turn);
        true
    }

    pub(crate) fn cancel_current(&self) -> Option<ChatMessageId> {
        let mut current = self.current.lock().expect("active turn lock poisoned");
        let turn = current.take()?;
        let message_id = turn.message_id;
        Self::cancel_active_turn(turn);
        Some(message_id)
    }

    pub(crate) fn clear(&self, message_id: ChatMessageId) {
        let mut current = self.current.lock().expect("active turn lock poisoned");
        if current
            .as_ref()
            .is_some_and(|turn| turn.message_id == message_id)
        {
            *current = None;
        }
    }

    pub(crate) fn cancel_active_turn(turn: ActiveMockTurn) {
        turn.cancel.cancel();
        if let Some(abort_handle) = turn.abort_handle {
            abort_handle.abort();
        }
    }
}

pub(crate) fn run_with_config_and_mock_token_delay(
    config: AgentConfig,
    mock_token_delay: Duration,
) -> Result<()> {
    run_with_config_mock_token_delay_and_compact_policy(
        config,
        mock_token_delay,
        CompactPolicy::default(),
    )
}

pub(crate) fn run_with_config_mock_token_delay_and_compact_policy(
    config: AgentConfig,
    mock_token_delay: Duration,
    compact_policy: CompactPolicy,
) -> Result<()> {
    let quit_events = EventQueue::new();
    let quit_events_for_menu = quit_events.clone();
    let quit_events_for_loop = quit_events.clone();
    let (action_sender, action_receiver) = EventQueue::<AppAction>::channel();
    let runtime = AgentRuntime::new(
        config,
        action_sender.clone(),
        MockTurnRegistry::with_token_delay(mock_token_delay),
    );
    restore_transcript_if_configured(
        &runtime.message_store,
        runtime.config.transcript_path.as_deref(),
    )?;
    append_startup_notices(&runtime.config, &runtime.message_store);
    runtime.transcript_status.sync(&runtime.message_store);
    let transcript_persistence = Arc::new(Mutex::new(TranscriptPersistence::new(
        runtime.config.transcript_path.clone(),
        &runtime.message_store,
    )));
    let runtime_for_build = runtime.clone();
    let runtime_for_actions = runtime.clone();
    let context_menus_for_loop = runtime.context_menus.clone();
    let persistence_for_actions = transcript_persistence.clone();
    let persistence_for_loop = transcript_persistence.clone();

    let run_result = run_crossterm_desktop_with_actions(
        CrosstermAppConfig::default()
            .bracketed_paste(true)
            .cursor(CursorMode::Show),
        move |screen| {
            Ok(AgentApp::with_runtime_state(
                screen,
                quit_events_for_menu,
                runtime_for_build.clone(),
                compact_policy,
            )
            .into_desktop())
        },
        action_receiver,
        move |_desktop, action, _screen| {
            let tool_runtime = runtime_for_actions.tool_runtime();
            let changed = apply_app_action(
                &runtime_for_actions.message_store,
                &runtime_for_actions.input_handle,
                &runtime_for_actions.mock_turns,
                &runtime_for_actions.status_state,
                &runtime_for_actions.transcript_status,
                &tool_runtime,
                action,
            );
            if changed {
                persistence_for_actions
                    .lock()
                    .expect("transcript persistence lock poisoned")
                    .save_if_dirty()?;
            }
            Ok(AppControl::Continue)
        },
        move |desktop, screen| {
            for request in context_menus_for_loop.drain() {
                desktop.add_window(
                    popup_menu_window(request.items, request.anchor, screen, "Message"),
                    screen,
                );
            }
            persistence_for_loop
                .lock()
                .expect("transcript persistence lock poisoned")
                .save_if_dirty()?;
            if quit_events_for_loop.pop().is_some() {
                Ok(AppControl::Exit)
            } else {
                Ok(AppControl::Continue)
            }
        },
        |_desktop, _event, _screen, _result| Ok(AppControl::Continue),
    );
    let save_result = transcript_persistence
        .lock()
        .expect("transcript persistence lock poisoned")
        .save_now();
    run_result.and(save_result)
}

pub(crate) fn restore_transcript_if_configured(
    store: &ChatMessageStore,
    path: Option<&Path>,
) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    let messages = load_transcript_jsonl(path)?;
    if !messages.is_empty() {
        store.replace_all(messages);
    }
    Ok(())
}

pub(crate) fn append_startup_notices(config: &AgentConfig, store: &ChatMessageStore) {
    if config.should_show_missing_api_key_prompt() && store.messages().is_empty() {
        append_system_message(store, MISSING_DEEPSEEK_API_KEY_NOTICE);
    }
}
