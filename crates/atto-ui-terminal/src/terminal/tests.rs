    use super::*;

    fn test_shared() -> TerminalShared {
        let runtime_config = TerminalRuntimeConfig::from_config(&TerminalConfig::default())
            .expect("default terminal config");
        TerminalShared {
            parser: terminal_parser(24, 80, runtime_config.scrollback_len),
            scrollback_len: runtime_config.scrollback_len,
            palette: runtime_config.palette,
            alternate_screen_scroll: runtime_config.alternate_screen_scroll,
            input: VecDeque::new(),
            on_input: None,
            input_forward: None,
            on_exit: None,
            on_window_title: None,
            on_window_icon_name: None,
            on_audible_bell: None,
            on_clipboard_copy: None,
            on_command_finished: None,
            system_clipboard: None,
            pty_resize: None,
            shell_integration: runtime_config.shell_integration,
            tmux_environment: runtime_config.tmux_environment,
            last_shell_integration_error: None,
            exit_status: None,
            process_running: false,
            window_title: None,
            window_icon_name: None,
            audible_bell_count: 0,
            last_clipboard_copy: None,
            last_system_clipboard_text: None,
            last_system_clipboard_error: None,
            capture: true,
            capture_suspended_by_blur: false,
            release_shortcut: runtime_config.release_shortcut,
            prefix_shortcut: runtime_config.prefix_shortcut,
            prefix_bindings: default_prefix_bindings(),
            prefix_pending: false,
            copy_mode: None,
            copy_buffer: None,
            selection: TerminalSelectionState::default(),
            command_marks: Vec::new(),
            current_cwd: None,
            cursor_shape: TerminalCursorShape::default(),
            dsr_tail: Vec::new(),
            tmux_dcs_passthrough: TmuxDcsPassthroughDecoder::default(),
        }
    }

    fn mouse_at(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn terminal_palette_from_theme_maps_terminal_colors() {
        let dark = TerminalPalette::from_theme(&Theme::dark());
        assert_eq!(dark.foreground, Some(Color::Gray));
        assert_eq!(dark.background, Some(Color::Rgb(16, 16, 16)));
        assert_eq!(dark.ansi[0], Color::Black);
        assert_eq!(dark.ansi[15], Color::White);

        let light = TerminalPalette::from_theme(&Theme::light());
        assert_eq!(light.foreground, Some(Color::Black));
        assert_eq!(light.background, Some(Color::Rgb(250, 250, 250)));
    }

    #[test]
    fn mouse_coords_local_uses_explicit_coordinate_space() {
        let area = Some(Rect::new(10, 5, 4, 3));

        assert_eq!(
            mouse_coords_local(area, mouse_at(11, 6), MouseCoordinateSpace::Absolute),
            Some((1, 1))
        );
        assert_eq!(
            mouse_coords_local(area, mouse_at(1, 1), MouseCoordinateSpace::Absolute),
            None
        );
        assert_eq!(
            mouse_coords_local(area, mouse_at(1, 1), MouseCoordinateSpace::Local),
            Some((1, 1))
        );
        assert_eq!(
            mouse_coords_local(area, mouse_at(11, 6), MouseCoordinateSpace::Local),
            None
        );
    }

    #[test]
    fn dsr_responses_handle_split_packets() {
        let mut shared = test_shared();

        assert!(collect_dsr_responses(&mut shared, b"\x1b[?6").is_empty());
        let responses = collect_dsr_responses(&mut shared, b"n");
        assert_eq!(responses, vec![b"\x1b[?1;1R".to_vec()]);

        assert!(collect_dsr_responses(&mut shared, b"\x1b[?").is_empty());
        let responses = collect_dsr_responses(&mut shared, b"5n");
        assert_eq!(responses, vec![b"\x1b[?0n".to_vec()]);
    }

    #[test]
    fn dsr_complete_packets_do_not_repeat_on_later_output() {
        let mut shared = test_shared();

        let responses = collect_dsr_responses(&mut shared, b"\x1b[6n");
        assert_eq!(responses, vec![b"\x1b[1;1R".to_vec()]);

        assert!(collect_dsr_responses(&mut shared, b"x").is_empty());
        assert!(shared.dsr_tail.is_empty());
    }

    #[test]
    fn dsr_tail_is_capped_against_unterminated_csi() {
        let mut shared = test_shared();

        // An unterminated CSI followed by an unbounded run of digits must not be
        // buffered without limit across chunks.
        let mut chunk = Vec::from(&b"\x1b["[..]);
        chunk.extend(std::iter::repeat_n(b'9', 4096));
        assert!(collect_dsr_responses(&mut shared, &chunk).is_empty());
        assert!(
            shared.dsr_tail.len() <= DSR_TAIL_MAX,
            "dsr_tail grew to {} bytes",
            shared.dsr_tail.len()
        );

        // Feeding another chunk keeps it bounded rather than accumulating.
        assert!(collect_dsr_responses(&mut shared, &chunk).is_empty());
        assert!(shared.dsr_tail.len() <= DSR_TAIL_MAX);
    }

    #[test]
    fn device_attribute_and_keyboard_queries_are_answered() {
        let mut shared = test_shared();

        // DA1: both `CSI c` and `CSI 0 c` forms.
        assert_eq!(
            collect_dsr_responses(&mut shared, b"\x1b[c"),
            vec![b"\x1b[?62c".to_vec()]
        );
        assert_eq!(
            collect_dsr_responses(&mut shared, b"\x1b[0c"),
            vec![b"\x1b[?62c".to_vec()]
        );
        // DA2.
        assert_eq!(
            collect_dsr_responses(&mut shared, b"\x1b[>c"),
            vec![b"\x1b[>0;0;0c".to_vec()]
        );
        // Kitty keyboard-protocol flags query.
        assert_eq!(
            collect_dsr_responses(&mut shared, b"\x1b[?u"),
            vec![b"\x1b[?0u".to_vec()]
        );
    }

    #[test]
    fn neovim_startup_query_batch_is_fully_answered() {
        let mut shared = test_shared();

        // The exact batch Neovim emits on startup (kitty flags, DA1, OSC 11
        // background query, DSR status). We must answer the keyboard query,
        // DA1, and DSR — OSC 11 is not a CSI query and is left to the parser.
        let responses = collect_dsr_responses(&mut shared, b"\x1b[?u\x1b[c\x1b]11;?\x07\x1b[5n");
        assert_eq!(
            responses,
            vec![
                b"\x1b[?0u".to_vec(),
                b"\x1b[?62c".to_vec(),
                b"\x1b[0n".to_vec(),
            ]
        );
    }

    #[test]
    fn da_query_split_across_chunks_is_buffered() {
        let mut shared = test_shared();

        assert!(collect_dsr_responses(&mut shared, b"\x1b[").is_empty());
        assert_eq!(
            collect_dsr_responses(&mut shared, b"c"),
            vec![b"\x1b[?62c".to_vec()]
        );
    }

    #[test]
    fn osc133_and_osc7_record_command_block_marks() {
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();

        handle.process_output_str(
            "\x1b]7;file://host/tmp/project%20one\x07\
             \x1b]133;A\x07$ echo ok\
             \x1b]133;B\x07\r\n\
             \x1b]133;C\x07ok\r\n\
             \x1b]133;D;7\x07",
        );

        let shared = terminal.shared.lock();
        assert_eq!(shared.current_cwd.as_deref(), Some("/tmp/project one"));
        assert_eq!(shared.command_marks.len(), 1);
        assert_eq!(
            shared.command_marks[0],
            TerminalCommandBlock {
                prompt_start: Some(0),
                prompt_start_col: Some(0),
                command_start: Some(0),
                command_start_col: Some(9),
                output_start: Some(1),
                output_start_col: Some(0),
                end: Some(2),
                end_col: Some(0),
                exit_code: Some(7),
                cwd: Some("/tmp/project one".to_string()),
            }
        );
    }

    #[test]
    fn osc133_zsh_preexec_order_still_captures_command_text() {
        // Real zsh/bash shell integration emits B (command-start) and C
        // (output-start) together from preexec/PS0, i.e. AFTER the user pressed
        // Enter — so both land at the output position, not inline after the
        // typed command. The typed command lives on the prompt line between A
        // and the newline. Copy command / Rerun must still recover it.
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();

        handle.process_output_str(
            "\x1b]133;A\x07$ echo hi\r\n\
             \x1b]133;B\x07\x1b]133;C\x07hi\r\n\
             \x1b]133;D;0\x07",
        );

        let command = handle
            .copy_command_block_command(0)
            .expect("command text recoverable");
        assert_eq!(command, "$ echo hi");

        let output = handle
            .copy_command_block_output(0)
            .expect("output text recoverable");
        assert_eq!(output, "hi");
    }

    #[test]
    fn osc133_missing_markers_degrade_to_partial_blocks() {
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();

        handle.process_output_str("plain output\r\n\x1b]133;D;0\x07");

        let shared = terminal.shared.lock();
        assert_eq!(shared.command_marks.len(), 1);
        assert_eq!(
            shared.command_marks[0],
            TerminalCommandBlock {
                prompt_start: Some(1),
                prompt_start_col: Some(0),
                end: Some(1),
                end_col: Some(0),
                exit_code: Some(0),
                ..TerminalCommandBlock::default()
            }
        );
    }

    #[test]
    fn clear_screen_drops_stale_command_marks() {
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();

        // Run one command to completion, leaving a recorded block on rows 0-2.
        handle.process_output_str(
            "\x1b]133;A\x07$ echo ok\
             \x1b]133;B\x07\x1b]133;C\x07ok\r\n\
             \x1b]133;D;0\x07",
        );
        assert_eq!(terminal.shared.lock().command_marks.len(), 1);

        // A bare Ctrl-L clears the screen and homes the cursor without any new
        // OSC 133 marker (zsh/bash `clear-screen` does not re-run precmd). The
        // stale block's rows are now blank, so it must be pruned immediately,
        // not only once the next command cycle emits a fresh prompt marker.
        handle.process_output_str("\x1b[H\x1b[2J");

        assert!(terminal.shared.lock().command_marks.is_empty());
    }

    #[test]
    fn clear_screen_keeps_marks_still_visible_on_screen() {
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();

        // Finished command whose rows remain populated on screen: an in-place
        // cursor repaint (no erase) must not drop the still-visible block.
        handle.process_output_str(
            "\x1b]133;A\x07$ echo ok\
             \x1b]133;B\x07\x1b]133;C\x07ok\r\n\
             \x1b]133;D;0\x07",
        );
        handle.process_output_str("\x1b[H$ echo ok");

        assert_eq!(terminal.shared.lock().command_marks.len(), 1);
    }

    #[test]
    fn osc7_decodes_file_uri_paths() {
        assert_eq!(
            parse_osc7_cwd(b"file://localhost/Users/test/project%20one").as_deref(),
            Some("/Users/test/project one")
        );
        assert_eq!(
            parse_osc7_cwd(b"file:///tmp/%E4%BD%A0%E5%A5%BD").as_deref(),
            Some("/tmp/你好")
        );
        assert_eq!(parse_osc7_cwd(b"https://example.invalid/tmp"), None);
    }

    #[test]
    fn shell_integration_defaults_to_zero_intrusion() {
        let terminal = TerminalEmulator::new();
        let handle = terminal.handle();
        let mut cmd = CommandBuilder::new("/bin/bash");

        let files = prepare_shell_integration(&mut cmd, handle.shell_integration())
            .expect("disabled shell integration is infallible");

        assert_eq!(
            handle.shell_integration(),
            TerminalShellIntegration::Disabled
        );
        assert!(files.is_none());
        assert_eq!(cmd.get_argv().as_slice(), [OsString::from("/bin/bash")]);
    }

    #[test]
    fn shell_integration_wraps_interactive_bash() {
        let mut cmd = CommandBuilder::new("/bin/bash");

        let files = prepare_shell_integration(&mut cmd, TerminalShellIntegration::enabled())
            .expect("prepare integration")
            .expect("bash integration files");

        assert_eq!(
            cmd.get_argv().as_slice(),
            [
                OsString::from("/bin/bash"),
                OsString::from("--rcfile"),
                files.entrypoint().as_os_str().to_os_string(),
                OsString::from("-i")
            ]
        );
        assert_eq!(
            cmd.get_env("ATTO_UI_SHELL_INTEGRATION"),
            Some(OsStr::new("1"))
        );
        assert!(
            fs::read_to_string(files.entrypoint())
                .expect("read bash integration script")
                .contains("OSC 133/7")
        );

        let root = files.root().to_path_buf();
        drop(files);
        assert!(!root.exists());
    }

    #[test]
    fn shell_integration_leaves_non_interactive_shell_commands_unchanged() {
        let mut cmd = CommandBuilder::new("/bin/bash");
        cmd.arg("-c");
        cmd.arg("echo unchanged");

        let files = prepare_shell_integration(&mut cmd, TerminalShellIntegration::enabled())
            .expect("prepare integration");

        assert!(files.is_none());
        assert_eq!(
            cmd.get_argv().as_slice(),
            [
                OsString::from("/bin/bash"),
                OsString::from("-c"),
                OsString::from("echo unchanged")
            ]
        );
        assert_eq!(cmd.get_env("ATTO_UI_SHELL_INTEGRATION"), None);
    }

    #[test]
    fn shell_integration_mode_is_queryable_and_mutable() {
        let terminal =
            TerminalEmulator::new().shell_integration(TerminalShellIntegration::enabled());
        let handle = terminal.handle();

        assert_eq!(
            handle.shell_integration(),
            TerminalShellIntegration::enabled()
        );
        handle.set_shell_integration(TerminalShellIntegration::Disabled);
        assert_eq!(
            handle.shell_integration(),
            TerminalShellIntegration::Disabled
        );
        assert_eq!(handle.last_shell_integration_error(), None);
    }

    #[test]
    fn tmux_environment_mode_is_queryable_and_mutable() {
        let terminal = TerminalEmulator::new().tmux_environment(TerminalTmuxEnvironmentConfig {
            inject: true,
            socket_path: "/tmp/atto-ui-builder.sock".to_string(),
            shim_path: None,
            server_pid: Some(1111),
            session_id: 2,
            pane_id: 4,
            override_term: false,
        });
        let handle = terminal.handle();

        assert_eq!(
            handle.tmux_environment().tmux_env_value(),
            "/tmp/atto-ui-builder.sock,1111,2"
        );
        handle.set_tmux_environment(TerminalTmuxEnvironmentConfig::default());
        assert!(!handle.tmux_environment().inject);
    }

    #[test]
    fn spawn_command_preparation_sets_terminal_env_and_default_cwd() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.env("TERM", "host-term");
        cmd.env("COLORTERM", "host-colorterm");
        cmd.env_remove("TMUX");
        cmd.env_remove("TMUX_PANE");

        prepare_spawn_command(&mut cmd, &TerminalTmuxEnvironmentConfig::default())
            .expect("prepare spawn command");

        assert_eq!(cmd.get_env("TERM"), Some(OsStr::new(DEFAULT_TERM_ENV)));
        assert_eq!(
            cmd.get_env("COLORTERM"),
            Some(OsStr::new(DEFAULT_COLORTERM_ENV))
        );
        assert_eq!(cmd.get_env("TMUX"), None);
        assert_eq!(cmd.get_env("TMUX_PANE"), None);
        assert_eq!(
            cmd.get_cwd().and_then(|cwd| cwd.to_str()),
            env::current_dir().expect("current dir").as_path().to_str()
        );
    }

    #[test]
    fn spawn_command_preparation_injects_tmux_probe_environment_when_enabled() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        let tmux = TerminalTmuxEnvironmentConfig {
            inject: true,
            socket_path: "/tmp/atto-ui-test.sock".to_string(),
            shim_path: None,
            server_pid: Some(4242),
            session_id: 7,
            pane_id: 3,
            override_term: false,
        };

        prepare_spawn_command(&mut cmd, &tmux).expect("prepare spawn command");

        assert_eq!(cmd.get_env("TERM"), Some(OsStr::new(DEFAULT_TERM_ENV)));
        assert_eq!(
            cmd.get_env("TMUX"),
            Some(OsStr::new("/tmp/atto-ui-test.sock,4242,7"))
        );
        assert_eq!(cmd.get_env("TMUX_PANE"), Some(OsStr::new("%3")));
    }

    #[test]
    fn spawn_command_preparation_prepends_tmux_shim_path_when_enabled() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.env("PATH", "/usr/bin:/bin");
        let tmux = TerminalTmuxEnvironmentConfig {
            inject: true,
            socket_path: "/tmp/atto-ui-test.sock".to_string(),
            shim_path: Some("/tmp/atto-ui-shim".to_string()),
            server_pid: Some(4242),
            session_id: 7,
            pane_id: 3,
            override_term: false,
        };

        prepare_spawn_command(&mut cmd, &tmux).expect("prepare spawn command");

        let path = cmd.get_env("PATH").expect("PATH env");
        let paths = env::split_paths(path).collect::<Vec<_>>();
        assert_eq!(paths.first(), Some(&PathBuf::from("/tmp/atto-ui-shim")));
        assert_eq!(paths.get(1), Some(&PathBuf::from("/usr/bin")));
        assert_eq!(paths.get(2), Some(&PathBuf::from("/bin")));
    }

    #[test]
    fn spawn_command_preparation_can_use_tmux_term_when_enabled() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        let tmux = TerminalTmuxEnvironmentConfig {
            inject: true,
            override_term: true,
            ..TerminalTmuxEnvironmentConfig::default()
        };

        prepare_spawn_command(&mut cmd, &tmux).expect("prepare spawn command");

        assert_eq!(cmd.get_env("TERM"), Some(OsStr::new(TMUX_TERM_ENV)));
        assert_eq!(cmd.get_env("TMUX_PANE"), Some(OsStr::new("%0")));
    }

    #[test]
    fn spawn_command_preparation_preserves_explicit_cwd() {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.cwd(OsStr::new("/tmp"));

        prepare_spawn_command(&mut cmd, &TerminalTmuxEnvironmentConfig::default())
            .expect("prepare spawn command");

        assert_eq!(cmd.get_cwd().and_then(|cwd| cwd.to_str()), Some("/tmp"));
    }

    #[test]
    fn system_clipboard_backend_uses_arboard_when_osc52_fails() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let osc52_calls = Arc::clone(&calls);
        let arboard_calls = Arc::clone(&calls);

        copy_text_with_backends(
            "copy me",
            move |text| {
                osc52_calls.lock().push(format!("osc52:{text}"));
                Err(anyhow!("osc52 failed"))
            },
            move |text| {
                arboard_calls.lock().push(format!("arboard:{text}"));
                Ok(())
            },
        )
        .expect("arboard fallback should succeed");

        assert_eq!(
            calls.lock().as_slice(),
            ["osc52:copy me", "arboard:copy me"]
        );
    }

    #[test]
    fn system_clipboard_backend_reports_when_both_paths_fail() {
        let error = copy_text_with_backends(
            "copy me",
            |_| Err(anyhow!("osc52 failed")),
            |_| Err(anyhow!("arboard failed")),
        )
        .expect_err("both backends should fail")
        .to_string();

        assert!(error.contains("osc52 failed"));
        assert!(error.contains("arboard failed"));
    }

    #[test]
    fn strip_bracketed_paste_markers_removes_embedded_terminators() {
        // Payload that tries to close paste mode early and inject a command.
        let hostile = b"safe\x1b[201~rm -rf /\x1b[200~more";
        let cleaned = strip_bracketed_paste_markers(hostile);
        assert_eq!(cleaned, b"saferm -rf /more");
        // No paste markers survive in the cleaned payload.
        assert!(!cleaned.windows(6).any(|w| w == b"\x1b[201~"));
        assert!(!cleaned.windows(6).any(|w| w == b"\x1b[200~"));
    }

    #[test]
    fn strip_bracketed_paste_markers_leaves_plain_text_untouched() {
        let plain = "héllo 👋 world".as_bytes();
        assert_eq!(strip_bracketed_paste_markers(plain), plain);
    }
