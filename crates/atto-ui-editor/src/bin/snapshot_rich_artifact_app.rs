use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use ratatui::layout::Rect;

use atto_ui::app::{
    AppControl, CrosstermAppConfig, CursorMode, Desktop, MenuBar, run_crossterm_desktop,
};
use atto_ui::reactive::EventQueue;
use atto_ui::wm::{Window, WindowKind};
use atto_ui_chat::{
    Artifact, ArtifactId, ArtifactKind, ArtifactViewer, ChatMessage, ChatMessageList,
    ChatMessageStore, ChatRole,
};
use atto_ui_editor::RichArtifactViewer;

fn main() -> Result<()> {
    let store = ChatMessageStore::new();
    let artifacts = seed_artifacts(&store);
    let open_artifacts: EventQueue<ArtifactId> = EventQueue::new();
    let list = ChatMessageList::new(store.clone())
        .wrap_width(56)
        .show_timestamps(false)
        .on_open_artifact({
            let open_artifacts = open_artifacts.clone();
            move |artifact_id| open_artifacts.push(artifact_id)
        });

    let app_cfg = CrosstermAppConfig::default()
        .mouse_capture(true)
        .cursor(CursorMode::Hide)
        .tick_rate(Duration::from_millis(16));

    run_crossterm_desktop(
        app_cfg,
        move |screen: Rect| {
            let mut desktop = Desktop::new(atto_ui::theme::Theme::dark(), MenuBar::new(vec![]));
            let work = Desktop::layout(screen).work_area;
            desktop.add_window(
                Window::new(
                    WindowKind::Normal,
                    "Rich Artifacts",
                    Rect {
                        x: work.x.saturating_add(2),
                        y: work.y.saturating_add(2),
                        width: 64.min(work.width.saturating_sub(2)).max(32),
                        height: 10.min(work.height.saturating_sub(2)).max(8),
                    },
                    Box::new(list),
                ),
                screen,
            );
            Ok(desktop)
        },
        |_desktop, _screen| Ok(AppControl::Continue),
        move |desktop, _event, screen, _res| {
            for artifact_id in open_artifacts.drain() {
                if let Some(artifact) = artifacts.get(&artifact_id).cloned() {
                    let mut viewer = RichArtifactViewer::new(desktop, screen);
                    viewer.open(artifact);
                }
            }
            Ok(AppControl::Continue)
        },
    )
}

fn seed_artifacts(store: &ChatMessageStore) -> HashMap<ArtifactId, Artifact> {
    let code_id = ArtifactId::new("code-main");
    let diff_id = ArtifactId::new("diff-main");

    store.push(ChatMessage::artifact(
        store.next_message_id(),
        ChatRole::Assistant,
        ArtifactKind::Code,
        code_id.clone(),
        "main.rs",
    ));
    store.push(ChatMessage::artifact(
        store.next_message_id(),
        ChatRole::Assistant,
        ArtifactKind::Diff,
        diff_id.clone(),
        "main.patch",
    ));

    let mut artifacts = HashMap::new();
    artifacts.insert(
        code_id.clone(),
        Artifact::new(
            code_id,
            ArtifactKind::Code,
            "main.rs",
            "fn main() {\n    println!(\"CODE-ARTIFACT\");\n}\n",
        ),
    );
    artifacts.insert(
        diff_id.clone(),
        Artifact::new(
            diff_id,
            ArtifactKind::Diff,
            "main.patch",
            "--- a/main.rs\n+++ b/main.rs\n@@ -1,3 +1,4 @@\n fn main() {\n-    println!(\"old\");\n+    println!(\"DIFF-ARTIFACT\");\n }\n",
        ),
    );
    artifacts
}
