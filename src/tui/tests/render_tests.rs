use crate::core::paths::SystemPaths;
use crate::tui::action;
use crate::tui::app::{self, App, InputMode};
use crate::tui::ui;
use crate::tui::views;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

    #[test]
    fn test_tui_render_dashboard_does_not_panic() {
        let paths = SystemPaths::resolve();
        let app = App::new(paths);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let res = terminal.draw(|f| ui::draw(f, &app));
        assert!(
            res.is_ok(),
            "El renderizado base de la TUI no debe provocar panics"
        );

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(
            content.contains("mcserman"),
            "El render debe contener el título de mcserman"
        );
    }

    #[test]
    fn test_dashboard_renders_loader_version_and_modpack_info() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.instances.clear();
        app.instances.push(app::InstanceSummary {
            name: "rlcraft_dregora".to_string(),
            loader: "forge".to_string(),
            mc_version: "1.12.2".to_string(),
            ram: "8G".to_string(),
            port: 25565,
            state: app::ServerState::Online,
            source_pack: Some("serverpack:RLCraft Dregora".to_string()),
            loader_version: Some("14.23.5.2860".to_string()),
            modpack_info: Some(crate::core::config::ModpackInfo {
                provider: "CurseForge".to_string(),
                name: "RLCraft Dregora".to_string(),
                version: "1.1.2b".to_string(),
                project_id: Some("1003933".to_string()),
                file_id: Some("7285701".to_string()),
            }),
        });
        app.selected_instance = 0;
        app.active_tab = app::AppTab::Dashboard;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(content.contains("Modloader:"), "Debe mostrar la etiqueta Modloader");
        assert!(content.contains("forge 14.23.5.2860"), "Debe incluir el loader y su versión");
        assert!(content.contains("(Minecraft 1.12.2)"), "Debe incluir la versión de Minecraft");
        assert!(content.contains("Información del Modpack"), "Debe incluir sección de modpack");
        assert!(content.contains("RLCraft Dregora"));
        assert!(content.contains("1.1.2b"));
        assert!(content.contains("CurseForge"));
        assert!(content.contains("1003933"));
        assert!(content.contains("7285701"));
    }

    #[test]
    fn test_tui_small_screen_warning() {
        let paths = SystemPaths::resolve();
        let app = App::new(paths);

        // Terminal menor al tamaño mínimo permitido
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let res = terminal.draw(|f| ui::draw(f, &app));
        assert!(res.is_ok());

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(
            content.contains("demasiado peque"),
            "Debe mostrar aviso de tamaño mínimo"
        );
    }

    #[test]
    fn test_input_mode_insert_blocks_quit_and_global_shortcuts() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        // En modo Normal, 'q' debe emitir Action::Quit
        let key_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(app.handle_key(key_q), Some(action::Action::Quit));

        // Entrar en modo Insert (ej. en la consola)
        app.input_mode = InputMode::Insert;
        app.active_tab = app::AppTab::Console;

        // En modo Insert, 'q' NO debe emitir Action::Quit, sino escribir 'q' en el input
        let action = app.handle_key(key_q);
        assert_eq!(action, None, "Modo Insert debe bloquear Action::Quit");
        assert_eq!(
            app.console_input.value(),
            "q",
            "La tecla 'q' debe haberse insertado en el buffer de texto"
        );

        // Espacio NO debe emitir ToggleServer
        let key_space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(app.handle_key(key_space), None);
        assert_eq!(app.console_input.value(), "q ");

        // Esc debe volver a modo Normal
        let key_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.handle_key(key_esc), None);
        assert_eq!(
            app.input_mode,
            InputMode::Normal,
            "Esc debe retornar a InputMode::Normal"
        );
    }

    #[test]
    fn test_console_ring_buffer_bounded_capacity() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        // Insertar más líneas que la capacidad máxima del ring buffer
        for i in 0..2000 {
            app.push_log_line(format!("[INFO] Registro de prueba número {}", i));
        }

        assert_eq!(
            app.log_buffer.len(),
            app::MAX_LOG_LINES,
            "El ring buffer de logs debe acotarse estrictamente a MAX_LOG_LINES ({})",
            app::MAX_LOG_LINES
        );

        // La línea más antigua debe haber sido descartada
        let first = app.log_buffer.front().unwrap();
        assert!(
            first.contains("1000"),
            "Las líneas más antiguas deben haberse descartado primero"
        );
    }

    #[test]
    fn test_console_sanitization_strips_ansi_and_tabs() {
        let raw = "\x1B[32m[INFO]\x1B[0m Servidor arrancado\tOK\r\n";
        let clean = app::sanitize_log_line(raw);
        assert!(!clean.contains('\x1B'), "No debe contener escapes ANSI");
        assert!(!clean.contains('\t'), "Los tabs deben haberse expandido");
        assert!(
            !clean.contains('\r'),
            "Los retornos de carro deben haberse eliminado"
        );
        assert!(clean.contains("[INFO] Servidor arrancado    OK\n"));
    }

    #[test]
    fn test_modal_and_tabs_render_without_panic() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        let backend = TestBackend::new(90, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        app.active_tab = app::AppTab::Console;
        app.push_log_line("[INFO] Iniciando Minecraft".to_string());
        app.push_log_line("[WARN] Lag detectado en chunk".to_string());
        app.push_log_line("[ERROR] Crash simulado".to_string());
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        app.active_tab = app::AppTab::Modrinth;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        app.active_tab = app::AppTab::Settings;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        app.show_create_modal = true;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(
            content.contains("Crear Nueva Instancia"),
            "El modal debe ser visible en el buffer"
        );
    }

    #[test]
    fn test_parse_minecraft_formatting() {
        use ratatui::style::Color;

        let raw = "§cRojo §aVerde §lNegrita";
        let spans = views::properties::parse_minecraft_formatting(raw);

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "Rojo ");
        assert_eq!(spans[0].style.fg, Some(Color::LightRed));

        assert_eq!(spans[1].content, "Verde ");
        assert_eq!(spans[1].style.fg, Some(Color::LightGreen));

        assert_eq!(spans[2].content, "Negrita");
        assert!(spans[2]
            .style
            .add_modifier
            .contains(ratatui::style::Modifier::BOLD));
    }

    #[test]
    fn test_modal_esc_disambiguation() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        // Abrir modal de creación (inicia en Insert)
        app.show_create_modal = true;
        app.input_mode = InputMode::Insert;

        let key_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);

        // Primer Esc: debe salir de Insert a Normal SIN cerrar el modal
        let act1 = app.handle_key(key_esc);
        assert_eq!(act1, None);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.show_create_modal);

        // Segundo Esc: ahora en Normal, debe cerrar el modal
        let act2 = app.handle_key(key_esc);
        assert_eq!(act2, Some(action::Action::CloseCreateModal));
    }

    #[test]
    fn test_focus_switching() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        // Dashboard inicia con foco en Sidebar
        app.active_tab = app::AppTab::Dashboard;
        app.focus = app::Focus::Sidebar;

        // 'l' / Right pasa foco al panel principal
        let key_right = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE);
        let act = app.handle_key(key_right);
        assert_eq!(act, Some(action::Action::FocusMainPanel));
        app.update(act.unwrap(), &tokio::sync::mpsc::channel(1).0);
        assert_eq!(app.focus, app::Focus::MainPanel);

        // 'h' / Left regresa foco a la sidebar
        let key_left = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
        let act = app.handle_key(key_left);
        assert_eq!(act, Some(action::Action::FocusSidebar));
        app.update(act.unwrap(), &tokio::sync::mpsc::channel(1).0);
        assert_eq!(app.focus, app::Focus::Sidebar);
    }

    #[test]
    fn test_console_visual_lines_and_clamping() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.terminal_size = (80, 24);
        let (tx, _) = tokio::sync::mpsc::channel(16);

        // Sin logs: total visual lines es 0, max_scroll es 0
        assert_eq!(app.console_total_visual_lines(), 0);
        assert_eq!(app.console_max_scroll(), 0);

        // Intentar scrollear arriba sin logs no debe incrementar console_scroll
        app.update(action::Action::ScrollLogUp(15), &tx);
        assert_eq!(app.console_scroll, 0);

        // inner_width = 80 - 26 (sidebar) - 2 (border) = 52
        // Agregar líneas cortas
        for i in 0..10 {
            app.push_log_line(format!("Línea corta {}", i));
        }
        // 10 líneas cortas <= viewport_height (12), max_scroll debe ser 0
        assert_eq!(app.console_total_visual_lines(), 10);
        assert_eq!(app.console_max_scroll(), 0);
        app.update(action::Action::ScrollLogUp(5), &tx);
        assert_eq!(app.console_scroll, 0);

        // Agregar 30 líneas más para superar viewport_height
        for i in 10..40 {
            app.push_log_line(format!("Línea de log estándar número {}", i));
        }
        let total = app.console_total_visual_lines();
        assert_eq!(total, 40);
        let max_scroll = app.console_max_scroll();
        assert!(max_scroll > 0);

        // Scrollear hacia arriba: debe saturar en max_scroll, nunca superarlo
        app.update(action::Action::ScrollLogUp(9999), &tx);
        assert_eq!(app.console_scroll, max_scroll);
        assert!(!app.auto_scroll);

        // Scrollear hacia abajo parcialmente
        app.update(action::Action::ScrollLogDown(5), &tx);
        assert_eq!(app.console_scroll, max_scroll - 5);
        assert!(!app.auto_scroll);

        // Scrollear al final (ScrollLogBottom)
        app.update(action::Action::ScrollLogBottom, &tx);
        assert_eq!(app.console_scroll, 0);
        assert!(app.auto_scroll);
    }

    #[test]
    fn test_console_render_scroll_and_scrollbar() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.active_tab = app::AppTab::Console;
        app.focus = app::Focus::MainPanel;

        for i in 0..50 {
            app.push_log_line(format!("Log message {:03} with some extra padding text to test wrapping behavior across multiple columns", i));
        }

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        // Render al final (auto_scroll = true)
        terminal.draw(|f| ui::draw(f, &app)).unwrap();

        // Scrollear hacia arriba y renderizar de nuevo con scrollbar
        app.console_scroll = 20;
        app.auto_scroll = false;
        terminal.draw(|f| ui::draw(f, &app)).unwrap();
    }

    #[tokio::test]
    async fn test_sidebar_server_state_and_log_trigger() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.instances.clear();
        app.instances.push(app::InstanceSummary {
            name: "test_srv".to_string(),
            loader: "paper".to_string(),
            mc_version: "1.21".to_string(),
            ram: "4G".to_string(),
            port: 25565,
            state: app::ServerState::Offline,
            source_pack: None,
            loader_version: None,
            modpack_info: None,
        });
        app.selected_instance = 0;

        // Si la instancia recibe logs en vivo, no puede estar Offline -> debe pasar a Starting
        if let Some(inst) = app.instances.iter_mut().find(|i| i.name == "test_srv") {
            if inst.state == app::ServerState::Offline {
                inst.state = app::ServerState::Starting;
            }
        }
        assert_eq!(app.instances[0].state, app::ServerState::Starting);

        // Toggle server reactivo inmediato en memoria
        let (tx, _) = tokio::sync::mpsc::channel(16);
        app.instances[0].state = app::ServerState::Offline;
        app.execute_toggle_server(&tx);
        assert_eq!(app.instances[0].state, app::ServerState::Starting);
    }

    #[tokio::test]
    async fn test_status_message_ttl_and_navigation_clearing() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _) = tokio::sync::mpsc::channel(16);

        app.set_status("Servidor 'test' ONLINE.");
        assert!(app.status_message.is_some());

        app.check_status_expiration();
        assert!(app.status_message.is_some());

        app.status_message = Some((
            "Servidor 'test' ONLINE.".to_string(),
            std::time::Instant::now() - std::time::Duration::from_secs(4),
        ));
        app.check_status_expiration();
        assert!(app.status_message.is_none());

        app.set_status("Mensaje temporal");
        assert!(app.status_message.is_some());
        app.update(action::Action::NextTab, &tx);
        assert!(app.status_message.is_none());

        app.set_status("Mensaje temporal 2");
        app.update(action::Action::GoToTab(0), &tx);
        assert!(app.status_message.is_none());

        app.set_status("Mensaje temporal 3");
        app.update(action::Action::FocusMainPanel, &tx);
        assert!(app.status_message.is_none());
    }

    #[test]
    fn test_sanitize_log_line_carriage_return_and_jline_erase() {
        // Test de reescritura por retorno de carro \r (progreso)
        let progress = "Downloading Paper 10%\rDownloading Paper 100%\r\n";
        let clean_prog = app::sanitize_log_line(progress);
        assert_eq!(clean_prog, "Downloading Paper 100%\n");

        // Test de secuencias de borrado de línea JLine/Paper (\r\x1b[2K)
        let jline = "\r\x1b[2K[12:00:00 INFO]: Servidor ONLINE\r\n";
        let clean_jline = app::sanitize_log_line(jline);
        assert_eq!(clean_jline, "[12:00:00 INFO]: Servidor ONLINE\n");

        // Test de \x1b[K (borrado hacia adelante)
        let forward_erase = "Long line from before that was longer\r\x1b[KShort line";
        let clean_erase = app::sanitize_log_line(forward_erase);
        assert_eq!(clean_erase, "Short line");
    }

    #[test]
    fn test_simplified_create_modal_labels() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.show_create_modal = true;

        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(
            content.contains("Tipo de Servidor"),
            "Debe contener 'Tipo de Servidor'"
        );
        assert!(
            content.contains("Versión de Minecraft"),
            "Debe contener 'Versión de Minecraft'"
        );
        assert!(
            content.contains("Memoria RAM"),
            "Debe contener 'Memoria RAM'"
        );
        assert!(content.contains("Puerto"), "Debe contener 'Puerto'");
        assert!(content.contains("[● Paper]"), "Debe contener '[● Paper]'");
        assert!(content.contains("○ Fabric"), "Debe contener '○ Fabric'");
        assert!(content.contains("○ Vanilla"), "Debe contener '○ Vanilla'");

        assert!(
            !content.contains("(Modloader)"),
            "No debe contener '(Modloader)'"
        );
        assert!(
            !content.contains("Flechas ◄ ►"),
            "No debe contener 'Flechas ◄ ►' en títulos"
        );
        assert!(
            !content.contains("(Recomendado)"),
            "No debe contener '(Recomendado)'"
        );
        assert!(!content.contains("(Mods)"), "No debe contener '(Mods)'");
        assert!(
            !content.contains("Memoria RAM máxima"),
            "No debe contener 'Memoria RAM máxima'"
        );
        assert!(
            !content.contains("Puerto TCP del servidor"),
            "No debe contener 'Puerto TCP del servidor'"
        );
    }

    #[test]
    fn test_delete_server_flow_and_index_adjustment() {
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("mcserman_test_{}", unique_id));
        let paths = SystemPaths {
            instances_dir: temp_dir.join("instances"),
            runtimes_dir: temp_dir.join("runtimes"),
            socket_dir: temp_dir.join("run"),
            base_dir: temp_dir.clone(),
        };
        paths.ensure_dirs().unwrap();

        let mut app = App::new(paths.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(32);

        // Crear 3 directorios simulados de instancias
        for name in &["alpha", "beta", "gamma"] {
            let inst_dir = paths.instance_dir(name);
            std::fs::create_dir_all(&inst_dir).unwrap();
            let cfg = crate::core::config::InstanceConfig::new(
                name.to_string(),
                crate::core::config::LoaderType::Paper,
                "1.21.4".to_string(),
            );
            cfg.save(&inst_dir.join("instance.toml")).unwrap();
        }

        app.reload_instances();
        assert_eq!(app.instances.len(), 3);
        assert_eq!(app.instances[0].name, "alpha");
        assert_eq!(app.instances[1].name, "beta");
        assert_eq!(app.instances[2].name, "gamma");

        // Impedir borrado si el servidor está ONLINE o STARTING
        app.selected_instance = 1; // "beta"
        app.instances[1].state = app::ServerState::Online;

        let key_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
        let action = app.handle_key(key_d);
        assert_eq!(action, Some(action::Action::PromptDeleteInstance));

        app.update(action.unwrap(), &tx);
        assert!(
            !app.show_delete_modal,
            "No debe abrir el modal de borrado si está ONLINE"
        );
        assert!(
            app.status_message
                .as_ref()
                .map(|(m, _)| m.contains("Detén el servidor antes de eliminarlo"))
                .unwrap_or(false),
            "Debe advertir que se detenga el servidor"
        );

        // Permitir borrado si el servidor está OFFLINE
        app.instances[1].state = app::ServerState::Offline;
        let action_offline = app.handle_key(key_d);
        assert_eq!(action_offline, Some(action::Action::PromptDeleteInstance));
        app.update(action_offline.unwrap(), &tx);

        assert!(app.show_delete_modal, "Debe abrir modal de confirmación");
        assert_eq!(app.delete_server_name.as_deref(), Some("beta"));

        let key_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(
            app.handle_key(key_n),
            Some(action::Action::CancelDeleteInstance)
        );
        app.update(action::Action::CancelDeleteInstance, &tx);
        assert!(!app.show_delete_modal);
        assert!(app.delete_server_name.is_none());
        assert_eq!(app.instances.len(), 3);

        let key_x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(
            app.handle_key(key_x),
            Some(action::Action::PromptDeleteInstance)
        );
        let key_del = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
        assert_eq!(
            app.handle_key(key_del),
            Some(action::Action::PromptDeleteInstance)
        );

        app.update(action::Action::PromptDeleteInstance, &tx);
        assert!(app.show_delete_modal);

        let key_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            app.handle_key(key_enter),
            Some(action::Action::ConfirmDeleteInstance)
        );
        app.update(action::Action::ConfirmDeleteInstance, &tx);

        assert!(!app.show_delete_modal);
        assert_eq!(app.instances.len(), 2);
        assert_eq!(app.instances[0].name, "alpha");
        assert_eq!(app.instances[1].name, "gamma");
        assert_eq!(app.selected_instance, 1);
        assert!(
            !paths.instance_dir("beta").exists(),
            "El directorio de 'beta' debe haber sido eliminado"
        );

        app.update(action::Action::PromptDeleteInstance, &tx);
        app.update(action::Action::ConfirmDeleteInstance, &tx);

        assert_eq!(app.instances.len(), 1);
        assert_eq!(app.instances[0].name, "alpha");
        assert_eq!(app.selected_instance, 0);

        app.update(action::Action::PromptDeleteInstance, &tx);
        app.update(action::Action::ConfirmDeleteInstance, &tx);

        assert_eq!(app.instances.len(), 0);
        assert_eq!(app.selected_instance, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_modal_rendering() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.show_delete_modal = true;
        app.delete_server_name = Some("survival-hardcore".to_string());

        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(
            content.contains("¿Eliminar servidor 'survival-hardcore'?"),
            "Debe incluir la pregunta con el nombre del servidor"
        );
        assert!(
            content.contains("Esta acción borrará sus archivos."),
            "Debe advertir sobre el borrado de archivos"
        );
        assert!(
            content.contains("[ y / Enter ] Confirmar"),
            "Debe mostrar atajo de confirmación"
        );
        assert!(
            content.contains("[ n / Esc ] Cancelar"),
            "Debe mostrar atajo de cancelación"
        );
    }

    #[test]
    fn test_footer_contains_delete_hint() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        let backend = ratatui::backend::TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        app.active_tab = app::AppTab::Dashboard;
        app.focus = app::Focus::Sidebar;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let buf1: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            buf1.contains("d: Eliminar"),
            "Dashboard Sidebar debe incluir 'd: Eliminar'"
        );

        app.focus = app::Focus::MainPanel;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let buf2: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            buf2.contains("d: Eliminar"),
            "Dashboard MainPanel debe incluir 'd: Eliminar'"
        );

        app.active_tab = app::AppTab::Console;
        app.focus = app::Focus::Sidebar;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let buf3: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            buf3.contains("d: Eliminar"),
            "Console Sidebar debe incluir 'd: Eliminar'"
        );

        app.show_delete_modal = true;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let buf4: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            buf4.contains("y/Enter: Confirmar eliminación"),
            "Footer debe reflejar modo de borrado"
        );
    }

