use crate::core::paths::SystemPaths;
use crate::tui::action;
use crate::tui::app::{self, App};
use crate::tui::ui;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;

    #[test]
    fn test_properties_common_vs_all_mode() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        // Preparamos propiedades comunes y propiedades raras/avanzadas
        app.properties_list = vec![
            ("motd".to_string(), "Minecraft Server".to_string()),
            ("server-port".to_string(), "25565".to_string()),
            ("difficulty".to_string(), "easy".to_string()),
            ("rcon.port".to_string(), "25575".to_string()), // No común
            ("broadcast-rcon-to-ops".to_string(), "true".to_string()), // No común
        ];

        // Modo común por defecto: solo incluye motd, server-port, difficulty (3)
        assert!(!app.properties_show_all);
        let filtered_common = app.filtered_properties();
        assert_eq!(filtered_common.len(), 3);
        assert_eq!(filtered_common[0].1 .0, "motd");
        assert_eq!(filtered_common[1].1 .0, "server-port");
        assert_eq!(filtered_common[2].1 .0, "difficulty");

        app.active_tab = app::AppTab::Settings;
        app.focus = app::Focus::MainPanel;
        let key_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = app.handle_key(key_a);
        assert_eq!(action, Some(action::Action::TogglePropertiesShowAll));
        app.update(action.unwrap(), &tx);

        assert!(app.properties_show_all);
        assert_eq!(app.filtered_properties().len(), 5);

        let action2 = app.handle_key(key_a);
        assert_eq!(action2, Some(action::Action::TogglePropertiesShowAll));
        app.update(action2.unwrap(), &tx);

        assert!(!app.properties_show_all);
        assert_eq!(app.filtered_properties().len(), 3);

        app.properties_search_input = tui_input::Input::new("rcon".to_string());
        let search_results = app.filtered_properties();
        assert_eq!(search_results.len(), 2);
        assert!(search_results.iter().any(|(_, (k, _))| k == "rcon.port"));
        assert!(search_results
            .iter()
            .any(|(_, (k, _))| k == "broadcast-rcon-to-ops"));
    }

    #[test]
    fn test_properties_search_filter_and_navigation() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        app.properties_list = vec![
            ("server-port".to_string(), "25565".to_string()),
            ("online-mode".to_string(), "true".to_string()),
            ("motd".to_string(), "A Minecraft Server".to_string()),
            ("difficulty".to_string(), "easy".to_string()),
            ("gamemode".to_string(), "survival".to_string()),
        ];

        assert_eq!(app.filtered_properties().len(), 5);

        // Filtro por clave insensible a mayúsculas
        app.properties_search_input = tui_input::Input::new("PORT".to_string());
        let filtered = app.filtered_properties();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1 .0, "server-port");

        // Filtro por valor
        app.properties_search_input = tui_input::Input::new("SURVIVAL".to_string());
        let filtered_val = app.filtered_properties();
        assert_eq!(filtered_val.len(), 1);
        assert_eq!(filtered_val[0].1 .0, "gamemode");

        app.active_tab = app::AppTab::Settings;
        app.focus = app::Focus::MainPanel;
        app.properties_search_input.reset();

        let slash_key = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        let action = app.handle_key(slash_key);
        assert_eq!(action, Some(action::Action::StartPropertiesSearch));
        app.update(action.unwrap(), &tx);
        assert!(app.properties_search_active);

        let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action_esc = app.handle_key(esc_key);
        assert_eq!(action_esc, Some(action::Action::CancelPropertiesSearch));
        app.update(action_esc.unwrap(), &tx);
        assert!(!app.properties_search_active);
        assert!(app.properties_search_input.value().is_empty());
    }

    #[test]
    fn test_properties_tab_full_width_rendering() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        app.active_tab = app::AppTab::Settings;
        app.focus = app::Focus::MainPanel;
        app.properties_list = vec![
            ("motd".to_string(), "Minecraft Server".to_string()),
            ("server-port".to_string(), "25565".to_string()),
            ("view-distance".to_string(), "10".to_string()),
        ];
        app.selected_property = 0;

        let backend = ratatui::backend::TestBackend::new(140, 35);
        let mut terminal = Terminal::new(backend).unwrap();

        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            !content.contains("Detalles de la Propiedad"),
            "El panel derecho debe haber sido removido"
        );
        assert!(
            content.contains("[Comunes] (Presiona 'a' para ver todas)"),
            "Debe indicar modo Comunes en el título"
        );

        app.update(action::Action::TogglePropertiesShowAll, &tx);
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let content_all: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            content_all.contains("[Todas] (Presiona 'a' para filtrar comunes)"),
            "Debe indicar modo Todas en el título"
        );

        assert!(
            content.contains("q: Salir"),
            "Footer debe contener 'q: Salir'"
        );
        assert!(
            content.contains("a: Todas/Comunes"),
            "Footer debe contener 'a: Todas/Comunes'"
        );
        assert!(
            content.contains("/: Buscar"),
            "Footer debe contener '/: Buscar'"
        );
        assert!(
            content.contains("Space: Alternar"),
            "Footer debe contener 'Space: Alternar'"
        );
        assert!(
            content.contains("Enter: Editar"),
            "Footer debe contener 'Enter: Editar'"
        );
        assert!(
            content.contains("s: Guardar"),
            "Footer debe contener 's: Guardar'"
        );
    }

    #[tokio::test]
    async fn test_create_modal_modpack_ui_and_resolution() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        // Abrir modal de creación
        app.update(action::Action::OpenCreateModal, &tx);
        assert!(app.show_create_modal);
        assert_eq!(app.create_loader_idx, 0);

        // Seleccionar loader Modpack (índice 3)
        app.update(action::Action::SelectWizardLoader(3), &tx);
        assert_eq!(app.create_loader_idx, 3);

        // Renderizar modal en TestBackend y verificar labels dinámicos
        let backend = ratatui::backend::TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        // Debe contener la opción Modpack en el selector
        assert!(
            buffer_content.contains("Modpack"),
            "El selector debe mostrar la opción Modpack"
        );
        // Debe mostrar el label dinámico de modpack en lugar de Versión de Minecraft
        assert!(
            buffer_content.contains("Modpack / Server Pack"),
            "Debe mostrar el prompt para CurseForge, Modrinth o .zip"
        );

        // Simular resolución exitosa de Modpack
        let mock_resolved = crate::modrinth::modpack::ResolvedModpack {
            pack_type: crate::modrinth::modpack::PackType::ModrinthMrpack,
            project_id: "better-mc".to_string(),
            project_title: "Better MC [FABRIC] BMC2".to_string(),
            version_id: "M5BnAIQy".to_string(),
            version_name: "v40".to_string(),
            suggested_instance_name: "better-mc-fabric-bmc2".to_string(),
            mc_version: "1.20.1".to_string(),
            loader: "fabric".to_string(),
            loader_version: Some("0.16.9".to_string()),
            mrpack_url: "https://example.com/test.mrpack".to_string(),
            mrpack_filename: "test.mrpack".to_string(),
            mrpack_sha512: None,
            mrpack_size: 1024,
            modpack_info: None,
        };

        app.handle_modpack_resolved(Ok(mock_resolved));

        // Verificar que autocompleta el nombre de la instancia si estaba vacío
        assert_eq!(app.create_name_input.value(), "better-mc-fabric-bmc2");
        // Verificar que genera el status de éxito
        assert!(app
            .create_modpack_status
            .as_ref()
            .unwrap()
            .contains("Better MC [FABRIC] BMC2"));
        assert!(app.resolved_modpack.is_some());

        // Renderizar nuevamente para verificar que el status se dibuja en pantalla
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let updated_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            updated_content.contains("Better MC [FABRIC] BMC2"),
            "El estado resuelto debe aparecer visualmente en el modal"
        );
    }

    #[test]
    fn test_format_file_size() {
        use crate::tui::views::modrinth::format_file_size;
        assert_eq!(format_file_size(500), "500 B");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_file_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_file_size(2 * 1024 * 1024 * 1024), "2.00 GB");
    }

    #[tokio::test]
    async fn test_mods_tab_serverpack_warning_banner() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.instances.clear();
        app.instances.push(app::InstanceSummary {
            name: "bmc2_srv".to_string(),
            loader: "fabric".to_string(),
            mc_version: "1.20.1".to_string(),
            ram: "6G".to_string(),
            port: 25565,
            state: app::ServerState::Offline,
            source_pack: Some("serverpack:Better MC [FABRIC] BMC2".to_string()),
            loader_version: None,
            modpack_info: None,
        });
        app.selected_instance = 0;
        app.active_tab = app::AppTab::Modrinth;
        app.focus = app::Focus::MainPanel;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            buffer_content.contains("Advertencia"),
            "Debe mostrar el banner de advertencia si la instancia es de un Serverpack"
        );
        assert!(
            buffer_content.contains("Serverpack oficial"),
            "Debe explicar el riesgo de incompatibilidad/desincronización"
        );

        // Sin source_pack no debe mostrar la advertencia
        app.instances[0].source_pack = None;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let updated_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            !updated_content.contains("Serverpack oficial"),
            "No debe mostrar la advertencia si la instancia es estándar"
        );
    }

    #[tokio::test]
    async fn test_mods_tab_installed_and_navigation() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        app.instances.clear();
        app.instances.push(app::InstanceSummary {
            name: "fabric_test".to_string(),
            loader: "fabric".to_string(),
            mc_version: "1.20.1".to_string(),
            ram: "4G".to_string(),
            port: 25565,
            state: app::ServerState::Offline,
            source_pack: None,
            loader_version: None,
            modpack_info: None,
        });
        app.selected_instance = 0;
        app.active_tab = app::AppTab::Modrinth;
        app.focus = app::Focus::MainPanel;

        app.installed_mods = vec![
            app::InstalledMod {
                file_name: "fabric-api-0.92.jar".to_string(),
                size_bytes: 2 * 1024 * 1024,
            },
            app::InstalledMod {
                file_name: "sodium-fabric-0.5.8.jar".to_string(),
                size_bytes: 512 * 1024,
            },
        ];

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(buffer_content.contains("fabric-api-0.92.jar"));
        assert!(buffer_content.contains("sodium-fabric-0.5.8.jar"));
        assert!(buffer_content.contains("2.0 MB"));
        assert!(buffer_content.contains("512.0 KB"));
        assert!(buffer_content.contains("Mods Instalados (2)"));

        // Navegación con Down y Up
        assert_eq!(app.selected_installed_mod, 0);
        app.update(action::Action::Down, &tx);
        assert_eq!(app.selected_installed_mod, 1);
        app.update(action::Action::Up, &tx);
        assert_eq!(app.selected_installed_mod, 0);

        // Probar scroll dinámico con 30 mods en terminal de altura reducida
        let backend_small = TestBackend::new(100, 20);
        let mut terminal_small = Terminal::new(backend_small).unwrap();

        app.installed_mods = (0..30)
            .map(|i| app::InstalledMod {
                file_name: format!("mod-{:02}.jar", i),
                size_bytes: 1024 * 1024,
            })
            .collect();
        app.selected_installed_mod = 0;

        // Al inicio debe renderizar mod-00.jar pero NO mod-25.jar
        terminal_small.draw(|f| ui::draw(f, &app)).unwrap();
        let buf0: String = terminal_small
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(buf0.contains("mod-00.jar"));
        assert!(!buf0.contains("mod-25.jar"));

        // Al navegar hacia abajo hasta mod-25
        for _ in 0..25 {
            app.update(action::Action::Down, &tx);
        }
        assert_eq!(app.selected_installed_mod, 25);

        // Al redibujar, la vista DEBE haberse desplazado (scrolling activo) y mostrar mod-25.jar
        terminal_small.draw(|f| ui::draw(f, &app)).unwrap();
        let buf25: String = terminal_small
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            buf25.contains("mod-25.jar"),
            "mod-25.jar debe ser visible en pantalla tras scroll"
        );
        assert!(
            !buf25.contains("mod-00.jar"),
            "mod-00.jar debe haberse ocultado por scroll"
        );

        // Al navegar hacia arriba de vuelta a mod-00
        for _ in 0..25 {
            app.update(action::Action::Up, &tx);
        }
        assert_eq!(app.selected_installed_mod, 0);
        terminal_small.draw(|f| ui::draw(f, &app)).unwrap();
        let buf_top: String = terminal_small
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(buf_top.contains("mod-00.jar"));
        assert!(!buf_top.contains("mod-25.jar"));
    }

    #[tokio::test]
    async fn test_mods_tab_add_and_delete_mod_modals() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        app.instances.clear();
        app.instances.push(app::InstanceSummary {
            name: "fabric_test".to_string(),
            loader: "fabric".to_string(),
            mc_version: "1.20.1".to_string(),
            ram: "4G".to_string(),
            port: 25565,
            state: app::ServerState::Offline,
            source_pack: None,
            loader_version: None,
            modpack_info: None,
        });
        app.selected_instance = 0;
        app.active_tab = app::AppTab::Modrinth;
        app.focus = app::Focus::MainPanel;

        app.installed_mods = vec![app::InstalledMod {
            file_name: "lithium-0.11.2.jar".to_string(),
            size_bytes: 350 * 1024,
        }];

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        // Modal de Añadir Mod ('u')
        app.update(action::Action::OpenAddModModal, &tx);
        assert!(app.show_add_mod_modal);
        assert_eq!(app.input_mode, app::InputMode::Insert);

        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let add_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(add_content.contains("Instalar Mod"));
        assert!(add_content.contains("URL o Slug"));

        app.update(action::Action::CloseAddModModal, &tx);
        assert!(!app.show_add_mod_modal);
        assert_eq!(app.input_mode, app::InputMode::Normal);

        // Modal de Borrar Mod ('d')
        app.selected_installed_mod = 0;
        app.update(action::Action::PromptDeleteMod, &tx);
        assert!(app.show_delete_mod_modal);
        assert_eq!(
            app.delete_mod_filename.as_deref(),
            Some("lithium-0.11.2.jar")
        );

        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let del_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(del_content.contains("Eliminar Mod"));
        assert!(del_content.contains("lithium-0.11.2.jar"));

        app.update(action::Action::CancelDeleteMod, &tx);
        assert!(!app.show_delete_mod_modal);
        assert!(app.delete_mod_filename.is_none());
    }

    #[tokio::test]
    async fn test_create_modal_enter_disambiguation_and_esc_cancel() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        // Abrir modal -> Inicia en modo normal de navegación
        app.update(action::Action::OpenCreateModal, &tx);
        assert!(app.show_create_modal);
        assert_eq!(app.input_mode, app::InputMode::Normal);
        assert_eq!(app.create_field_idx, 0);

        // En modo Normal, 'Enter' emite SubmitCreateInstance
        let enter_key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = app.handle_key(enter_key);
        assert_eq!(action, Some(action::Action::SubmitCreateInstance));
        assert_eq!(app.input_mode, app::InputMode::Normal);

        // Edición exclusiva con 'i': Inicia modo edición y guarda backup
        let i_key = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
        let action = app.handle_key(i_key);
        assert_eq!(action, None);
        assert_eq!(app.input_mode, app::InputMode::Insert);
        assert!(app.create_input_backup.is_some());

        let char_key = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE);
        app.handle_key(char_key);
        let char_key2 = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
        app.handle_key(char_key2);
        assert_eq!(app.create_name_input.value(), "mc");

        // 'Esc' durante edición debe CANCELAR y restaurar el valor previo
        let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = app.handle_key(esc_key);
        assert_eq!(action, None);
        assert_eq!(app.input_mode, app::InputMode::Normal);
        assert_eq!(app.create_name_input.value(), "");

        // Entrar a editar de nuevo, escribir y confirmar con 'Enter'
        app.handle_key(i_key);
        assert_eq!(app.input_mode, app::InputMode::Insert);
        app.handle_key(char_key);
        app.handle_key(char_key2);
        assert_eq!(app.create_name_input.value(), "mc");

        let action = app.handle_key(enter_key);
        assert_eq!(action, None);
        assert_eq!(app.input_mode, app::InputMode::Normal);
        assert_eq!(app.create_name_input.value(), "mc");

        // 'Esc' en modo normal debe CERRAR el modal
        let action = app.handle_key(esc_key);
        assert_eq!(action, Some(action::Action::CloseCreateModal));

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(content.contains("i: Editar"));
        assert!(content.contains("Enter: Crear"));
        assert!(!content.contains("i/Enter: Editar"));
    }

    #[tokio::test]
    async fn test_addons_subfolder_and_tab3_footer() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.instances.clear();
        app.instances.push(app::InstanceSummary {
            name: "paper_srv".to_string(),
            loader: "paper".to_string(),
            mc_version: "1.21".to_string(),
            ram: "4G".to_string(),
            port: 25565,
            state: app::ServerState::Offline,
            source_pack: None,
            loader_version: None,
            modpack_info: None,
        });
        app.instances.push(app::InstanceSummary {
            name: "fabric_srv".to_string(),
            loader: "fabric".to_string(),
            mc_version: "1.20.1".to_string(),
            ram: "4G".to_string(),
            port: 25566,
            state: app::ServerState::Offline,
            source_pack: None,
            loader_version: None,
            modpack_info: None,
        });

        // Verificar detección automática de carpeta destino
        assert_eq!(app.instances[0].addons_subfolder(), "plugins");
        assert_eq!(app.instances[1].addons_subfolder(), "mods");

        // Verificar footer del Tab 3
        app.selected_instance = 0;
        app.active_tab = app::AppTab::Modrinth;
        app.focus = app::Focus::MainPanel;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let footer_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            footer_content.contains("u: Instalar por URL"),
            "Footer debe contener 'u: Instalar por URL'"
        );
        assert!(
            footer_content.contains("d: Eliminar"),
            "Footer debe contener 'd: Eliminar'"
        );
    }
