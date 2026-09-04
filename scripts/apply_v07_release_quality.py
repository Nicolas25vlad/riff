from pathlib import Path


def replace(path: str, old: str, new: str) -> None:
    target = Path(path)
    source = target.read_text()
    if old not in source:
        raise SystemExit(f"missing pattern in {path}: {old[:100]!r}")
    target.write_text(source.replace(old, new, 1))


replace("install.sh", 'cargo install --git "$REPO" --force', 'cargo install --git "$REPO" --locked --force')
replace("install.ps1", "cargo install --git $Repo --force", "cargo install --git $Repo --locked --force")

path = Path("src/workbench/mod.rs")
source = path.read_text()
marker = '''    #[test]\n    fn formats_transport_time() {\n'''
tests = r'''    fn test_workbench() -> Workbench {
        let item = QueueItem {
            title: "War Pigs".into(),
            artist: "Black Sabbath".into(),
            album: "Paranoid".into(),
            version: None,
            uri: "spotify:track:test".into(),
            duration_ms: 475_000,
            cover_id: None,
            match_score: Some(100),
        };
        Workbench {
            state: AppState {
                file_path: PathBuf::from("coding-metal.riff"),
                file_name: "coding-metal.riff".into(),
                playlist_name: "coding-metal".into(),
                queue: vec![item.clone()],
                transient_current: None,
                status: PlaybackStatus::Playing,
                current_uri: Some(item.uri.clone()),
                position_ms: 42_000,
                volume: volume_from_percent(65),
                shuffle: false,
                repeat: false,
                message: "ready".into(),
                view: View::NowPlaying,
                theme: Theme::from_env(),
                git: None,
                search: SearchState::default(),
                lyrics: LyricsState::default(),
                hits: HitMap::default(),
            },
            editor: EditorState {
                lines: vec!["playlist \"coding-metal\" {".into(), "}".into()],
                row: 0,
                col: 0,
                scroll: 0,
                dirty: false,
                clipboard: None,
                message: String::new(),
            },
            artwork: HashMap::new(),
            artwork_pending: HashMap::new(),
        }
    }

    fn render_virtual_terminal(width: u16, height: u16) -> Workbench {
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut workbench = test_workbench();
        terminal.draw(|frame| draw(frame, &mut workbench)).unwrap();
        workbench
    }

    #[test]
    fn renders_wide_layout_in_virtual_terminal() {
        let workbench = render_virtual_terminal(120, 35);
        assert_eq!(workbench.state.hits.tabs.len(), View::ALL.len());
        assert!(workbench.state.hits.volume.is_some());
        assert!(workbench.state.hits.progress.is_some());
    }

    #[test]
    fn renders_compact_layout_in_virtual_terminal() {
        let workbench = render_virtual_terminal(60, 20);
        let bounds = Rect::new(0, 0, 60, 20);
        for (_, tab) in &workbench.state.hits.tabs {
            assert!(tab.right() <= bounds.right());
            assert!(tab.bottom() <= bounds.bottom());
        }
        assert!(workbench.state.hits.progress.is_some());
    }

''' + marker
if marker not in source:
    raise SystemExit("missing Workbench test marker")
path.write_text(source.replace(marker, tests, 1))

replace(
    "scripts/check-tui-quality.sh",
    "echo 'TUI quality invariants passed.'",
    "echo 'TUI terminal ownership, input and virtual-render invariants passed.'",
)
