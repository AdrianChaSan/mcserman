use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_input::backend::crossterm::EventHandler;

use super::{is_boolean_property, is_enum_property, App, AppTab, Focus, InputMode};
use crate::tui::action::Action;

impl App {
    /// Manejo de teclas desacoplado con máquina de estados estricta (Rule 1).
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        // Si la búsqueda en propiedades (Tab 4) está activa
        if self.properties_search_active {
            match (key.modifiers, key.code) {
                (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Some(Action::Quit),
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    return Some(Action::CancelPropertiesSearch);
                }
                (KeyModifiers::NONE, KeyCode::Enter) => {
                    return Some(Action::ConfirmPropertiesSearch);
                }
                _ => {
                    self.properties_search_input
                        .handle_event(&crossterm::event::Event::Key(key));
                    self.selected_property = 0;
                    return None;
                }
            }
        }

        // Si el modal de confirmación de eliminación de instancia está abierto
        if self.show_delete_modal {
            match (key.modifiers, key.code) {
                (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Some(Action::Quit),
                (m, KeyCode::Char('y') | KeyCode::Char('Y'))
                    if !m.contains(KeyModifiers::CONTROL) =>
                {
                    return Some(Action::ConfirmDeleteInstance);
                }
                (KeyModifiers::NONE, KeyCode::Enter) => {
                    return Some(Action::ConfirmDeleteInstance);
                }
                (m, KeyCode::Char('n') | KeyCode::Char('N'))
                    if !m.contains(KeyModifiers::CONTROL) =>
                {
                    return Some(Action::CancelDeleteInstance);
                }
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    return Some(Action::CancelDeleteInstance);
                }
                _ => return None,
            }
        }

        // Si el modal de confirmación de eliminación de MOD está abierto
        if self.show_delete_mod_modal {
            match (key.modifiers, key.code) {
                (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Some(Action::Quit),
                (m, KeyCode::Char('y') | KeyCode::Char('Y'))
                    if !m.contains(KeyModifiers::CONTROL) =>
                {
                    return Some(Action::ConfirmDeleteMod);
                }
                (KeyModifiers::NONE, KeyCode::Enter) => {
                    return Some(Action::ConfirmDeleteMod);
                }
                (m, KeyCode::Char('n') | KeyCode::Char('N'))
                    if !m.contains(KeyModifiers::CONTROL) =>
                {
                    return Some(Action::CancelDeleteMod);
                }
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    return Some(Action::CancelDeleteMod);
                }
                _ => return None,
            }
        }

        // Si el modal de añadir mod vía URL/Slug está abierto
        if self.show_add_mod_modal {
            match (key.modifiers, key.code) {
                (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Some(Action::Quit),
                (KeyModifiers::NONE, KeyCode::Esc) => return Some(Action::CloseAddModModal),
                (KeyModifiers::NONE, KeyCode::Enter) => return Some(Action::SubmitAddModUrl),
                _ => {
                    self.add_mod_input
                        .handle_event(&crossterm::event::Event::Key(key));
                    return None;
                }
            }
        }

        // Si el modal de creación está abierto
        if self.show_create_modal {
            if self.input_mode == InputMode::Insert {
                match (key.modifiers, key.code) {
                    (KeyModifiers::NONE, KeyCode::Esc) => {
                        // Cancelar edición: restaurar valor previo si existe
                        if let Some(prev) = self.create_input_backup.take() {
                            match self.create_field_idx {
                                0 => self.create_name_input = tui_input::Input::new(prev),
                                2 => {
                                    if self.create_loader_idx == 3 {
                                        self.create_modpack_input = tui_input::Input::new(prev);
                                    }
                                }
                                3 => self.create_ram_input = tui_input::Input::new(prev),
                                4 => self.create_port_input = tui_input::Input::new(prev),
                                _ => {}
                            }
                        }
                        self.input_mode = InputMode::Normal;
                        return None;
                    }
                    (KeyModifiers::NONE, KeyCode::Enter) => {
                        // Confirmar edición y volver a modo navegación del modal
                        self.create_input_backup = None;
                        self.input_mode = InputMode::Normal;
                        if self.create_loader_idx == 3 && self.create_field_idx == 2 {
                            return Some(Action::ResolveModpack);
                        }
                        return None;
                    }
                    (KeyModifiers::NONE, KeyCode::Tab) => {
                        self.create_input_backup = None;
                        let prev = self.create_field_idx;
                        self.create_field_idx = (self.create_field_idx + 1) % 5;
                        if self.create_loader_idx == 3 && prev == 2 {
                            return Some(Action::ResolveModpack);
                        }
                        let is_text = if self.create_loader_idx == 3 {
                            matches!(self.create_field_idx, 0 | 2 | 3 | 4)
                        } else {
                            matches!(self.create_field_idx, 0 | 3 | 4)
                        };
                        if is_text {
                            self.create_input_backup = Some(match self.create_field_idx {
                                0 => self.create_name_input.value().to_string(),
                                2 => self.create_modpack_input.value().to_string(),
                                3 => self.create_ram_input.value().to_string(),
                                4 => self.create_port_input.value().to_string(),
                                _ => String::new(),
                            });
                        } else {
                            self.input_mode = InputMode::Normal;
                        }
                        return None;
                    }
                    (KeyModifiers::SHIFT, KeyCode::BackTab) => {
                        self.create_input_backup = None;
                        let prev = self.create_field_idx;
                        self.create_field_idx = if self.create_field_idx == 0 {
                            4
                        } else {
                            self.create_field_idx - 1
                        };
                        if self.create_loader_idx == 3 && prev == 2 {
                            return Some(Action::ResolveModpack);
                        }
                        let is_text = if self.create_loader_idx == 3 {
                            matches!(self.create_field_idx, 0 | 2 | 3 | 4)
                        } else {
                            matches!(self.create_field_idx, 0 | 3 | 4)
                        };
                        if is_text {
                            self.create_input_backup = Some(match self.create_field_idx {
                                0 => self.create_name_input.value().to_string(),
                                2 => self.create_modpack_input.value().to_string(),
                                3 => self.create_ram_input.value().to_string(),
                                4 => self.create_port_input.value().to_string(),
                                _ => String::new(),
                            });
                        } else {
                            self.input_mode = InputMode::Normal;
                        }
                        return None;
                    }
                    _ => {
                        match self.create_field_idx {
                            0 => {
                                self.create_name_input
                                    .handle_event(&crossterm::event::Event::Key(key));
                            }
                            2 => {
                                if self.create_loader_idx == 3 {
                                    self.create_modpack_input
                                        .handle_event(&crossterm::event::Event::Key(key));
                                    self.resolved_modpack = None;
                                    self.create_modpack_status = None;
                                }
                            }
                            3 => {
                                self.create_ram_input
                                    .handle_event(&crossterm::event::Event::Key(key));
                            }
                            4 => {
                                self.create_port_input
                                    .handle_event(&crossterm::event::Event::Key(key));
                            }
                            _ => {}
                        }
                        return None;
                    }
                }
            } else {
                // Modo Normal dentro del modal
                match (key.modifiers, key.code) {
                    (KeyModifiers::NONE, KeyCode::Esc) => {
                        self.create_input_backup = None;
                        return Some(Action::CloseCreateModal);
                    }
                    (KeyModifiers::NONE, KeyCode::Tab) | (KeyModifiers::NONE, KeyCode::Down) => {
                        return Some(Action::NextWizardField);
                    }
                    (KeyModifiers::SHIFT, KeyCode::BackTab) | (KeyModifiers::NONE, KeyCode::Up) => {
                        return Some(Action::PrevWizardField);
                    }
                    (KeyModifiers::NONE, KeyCode::Enter) => {
                        self.create_input_backup = None;
                        return Some(Action::SubmitCreateInstance);
                    }
                    (KeyModifiers::NONE, KeyCode::Char('i')) => {
                        let is_text = if self.create_loader_idx == 3 {
                            matches!(self.create_field_idx, 0 | 2 | 3 | 4)
                        } else {
                            matches!(self.create_field_idx, 0 | 3 | 4)
                        };
                        if is_text {
                            self.create_input_backup = Some(match self.create_field_idx {
                                0 => self.create_name_input.value().to_string(),
                                2 => self.create_modpack_input.value().to_string(),
                                3 => self.create_ram_input.value().to_string(),
                                4 => self.create_port_input.value().to_string(),
                                _ => String::new(),
                            });
                            self.input_mode = InputMode::Insert;
                        }
                        return None;
                    }
                    _ => {
                        if self.create_field_idx == 1 {
                            match key.code {
                                KeyCode::Left => {
                                    let prev = if self.create_loader_idx == 0 {
                                        3
                                    } else {
                                        self.create_loader_idx - 1
                                    };
                                    return Some(Action::SelectWizardLoader(prev));
                                }
                                KeyCode::Right => {
                                    let next = (self.create_loader_idx + 1) % 4;
                                    return Some(Action::SelectWizardLoader(next));
                                }
                                _ => {}
                            }
                        }
                        if self.create_field_idx == 2 && self.create_loader_idx != 3 {
                            match key.code {
                                KeyCode::Left => {
                                    if !self.create_versions.is_empty()
                                        && self.create_version_idx > 0
                                    {
                                        self.create_version_idx -= 1;
                                    }
                                    return None;
                                }
                                KeyCode::Right => {
                                    if !self.create_versions.is_empty()
                                        && self.create_version_idx + 1 < self.create_versions.len()
                                    {
                                        self.create_version_idx += 1;
                                    }
                                    return None;
                                }
                                _ => {}
                            }
                        }
                        return None;
                    }
                }
            }
        }

        // MODO INSERT: Escribiendo en consola, Modrinth o editando propiedad
        if self.input_mode == InputMode::Insert {
            match (key.modifiers, key.code) {
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    if self.editing_property.is_some() {
                        return Some(Action::CancelEditProperty);
                    }
                    self.input_mode = InputMode::Normal;
                    None
                }
                (KeyModifiers::NONE, KeyCode::Enter) => match self.active_tab {
                    AppTab::Console => Some(Action::SubmitConsoleCommand),
                    AppTab::Settings => {
                        if self.editing_property.is_some() {
                            Some(Action::SubmitEditProperty)
                        } else {
                            None
                        }
                    }
                    _ => None,
                },
                (KeyModifiers::NONE, KeyCode::PageUp) => match self.active_tab {
                    AppTab::Console => Some(Action::ScrollLogUp(15)),
                    _ => None,
                },
                (KeyModifiers::NONE, KeyCode::PageDown) => match self.active_tab {
                    AppTab::Console => Some(Action::ScrollLogDown(15)),
                    _ => None,
                },
                _ => {
                    match self.active_tab {
                        AppTab::Console => {
                            self.console_input
                                .handle_event(&crossterm::event::Event::Key(key));
                        }
                        AppTab::Settings => {
                            if let Some(ref mut input) = self.editing_property {
                                input.handle_event(&crossterm::event::Event::Key(key));
                            }
                        }
                        _ => {}
                    }
                    None
                }
            }
        } else {
            // MODO NORMAL: Navegación global activa
            match (key.modifiers, key.code) {
                (KeyModifiers::CONTROL, KeyCode::Char('c'))
                | (KeyModifiers::NONE, KeyCode::Char('q')) => Some(Action::Quit),
                (KeyModifiers::NONE, KeyCode::Char('h')) | (KeyModifiers::NONE, KeyCode::Left) => {
                    Some(Action::FocusSidebar)
                }
                (KeyModifiers::NONE, KeyCode::Char('l')) | (KeyModifiers::NONE, KeyCode::Right) => {
                    Some(Action::FocusMainPanel)
                }
                (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
                    Some(Action::Down)
                }
                (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
                    Some(Action::Up)
                }
                (KeyModifiers::NONE, KeyCode::Tab) => Some(Action::NextTab),
                (KeyModifiers::SHIFT, KeyCode::BackTab) => Some(Action::PrevTab),
                (KeyModifiers::NONE, KeyCode::Char('1')) => Some(Action::GoToTab(0)),
                (KeyModifiers::NONE, KeyCode::Char('2')) => Some(Action::GoToTab(1)),
                (KeyModifiers::NONE, KeyCode::Char('3')) => Some(Action::GoToTab(2)),
                (KeyModifiers::NONE, KeyCode::Char('4')) => Some(Action::GoToTab(3)),
                (KeyModifiers::NONE, KeyCode::Char('r')) => Some(Action::RefreshInstances),
                (KeyModifiers::NONE, KeyCode::Char('n')) => Some(Action::OpenCreateModal),
                (KeyModifiers::NONE, KeyCode::Char('u')) => {
                    if self.active_tab == AppTab::Modrinth {
                        Some(Action::OpenAddModModal)
                    } else {
                        None
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('d'))
                | (KeyModifiers::NONE, KeyCode::Char('x'))
                | (KeyModifiers::NONE, KeyCode::Delete) => {
                    if self.focus == Focus::Sidebar || self.active_tab == AppTab::Dashboard {
                        Some(Action::PromptDeleteInstance)
                    } else if self.active_tab == AppTab::Modrinth {
                        Some(Action::PromptDeleteMod)
                    } else {
                        None
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('/')) => {
                    if self.active_tab == AppTab::Settings {
                        Some(Action::StartPropertiesSearch)
                    } else {
                        None
                    }
                }
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    if self.active_tab == AppTab::Settings
                        && !self.properties_search_input.value().is_empty()
                    {
                        Some(Action::CancelPropertiesSearch)
                    } else {
                        None
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('a')) => {
                    if self.active_tab == AppTab::Settings {
                        Some(Action::TogglePropertiesShowAll)
                    } else {
                        None
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char(' ')) => {
                    if self.focus == Focus::MainPanel && self.active_tab == AppTab::Settings {
                        Some(Action::ToggleProperty)
                    } else {
                        Some(Action::ToggleServer)
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('i')) => match self.active_tab {
                    AppTab::Console => {
                        self.focus = Focus::MainPanel;
                        self.input_mode = InputMode::Insert;
                        None
                    }
                    AppTab::Modrinth => {
                        self.focus = Focus::MainPanel;
                        Some(Action::OpenAddModModal)
                    }
                    AppTab::Settings => {
                        self.focus = Focus::MainPanel;
                        Some(Action::StartEditProperty)
                    }
                    _ => None,
                },
                (KeyModifiers::NONE, KeyCode::Enter) => {
                    if self.focus == Focus::MainPanel {
                        match self.active_tab {
                            AppTab::Console => {
                                self.input_mode = InputMode::Insert;
                                None
                            }
                            AppTab::Modrinth => None,
                            AppTab::Settings => {
                                let filtered = self.filtered_properties();
                                if let Some(&(_, (k, v))) = filtered.get(self.selected_property) {
                                    if is_boolean_property(k, v) || is_enum_property(k) {
                                        Some(Action::ToggleProperty)
                                    } else {
                                        Some(Action::StartEditProperty)
                                    }
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    } else {
                        match self.active_tab {
                            AppTab::Console | AppTab::Modrinth => {
                                self.focus = Focus::MainPanel;
                                self.input_mode = InputMode::Insert;
                                None
                            }
                            _ => None,
                        }
                    }
                }
                (KeyModifiers::NONE, KeyCode::PageUp) => Some(Action::ScrollLogUp(15)),
                (KeyModifiers::NONE, KeyCode::PageDown) => Some(Action::ScrollLogDown(15)),
                (KeyModifiers::NONE, KeyCode::Home) => match self.active_tab {
                    AppTab::Console => Some(Action::ScrollLogUp(usize::MAX)),
                    _ => None,
                },
                (KeyModifiers::NONE, KeyCode::End) => match self.active_tab {
                    AppTab::Console => Some(Action::ScrollLogBottom),
                    _ => None,
                },
                (KeyModifiers::NONE, KeyCode::Char('s')) => {
                    if self.active_tab == AppTab::Settings {
                        Some(Action::SaveProperties)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
    }

}
