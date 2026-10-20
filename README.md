# ⛏️ mcserman (mc-server-manager)

> **A high-performance, host-independent Minecraft Dedicated Server Manager in Rust with full Ratatui TUI and headless CLI.**

`mcserman` allows you to create, manage, configure, and supervise multi-instance Minecraft servers with zero external bloat. It features an interactive **Ratatui TUI dashboard**, native Unix domain socket supervision, multi-JDK management, Modrinth API integration, and first-class **NixOS** flake support.

---

## ✨ Key Features

- 🖥️ **Full Interactive TUI (Ratatui & Crossterm):** Modern terminal dashboard with sidebar instance selection, real-time metrics, live log streaming with auto-scroll, in-TUI Modrinth mod/plugin browser, server.properties editor, and modal creation wizards.
- ⚡ **Dual CLI & TUI Architecture:** Running `mcserman` in an interactive terminal launches the TUI; running subcommands (`start`, `stop`, `daemon`, etc.) operates headlessly with zero UI overhead for scripts, CI/CD, and `systemd`.
- 🔌 **Hardened Daemon Supervision (`setsid`):** JVM runs in a detached session (`setsid`) with `Stdio::null()`, preventing terminal hangup signals (`SIGHUP`) from killing servers and leveraging an asynchronous process reaper to eliminate zombie (`<defunct>`) processes.
- ☕ **Smart Multi-JDK Resolution:** Out-of-the-box support for Java 21, 17, and 8. Automatically detects Nix Store paths (`MCSERVER_JAVA_*_PATH`), system `$PATH`, or falls back to downloading Eclipse Temurin JDKs.
- 📦 **Multi-Loader Support:** Automated provisioning for Mojang Vanilla, PaperMC (automatic latest build detection via v3 API), and Fabric Loader.
- 🧩 **Modrinth Integration:** Search and download compatible mods and plugins directly with throttled progress reporting and cryptographic SHA-512 verification.
- ❄️ **Hermetic NixOS Support:** Flake exports `packages.default` (wrapped with Temurin 21/17/8), `devShells.default` for rapid development, and `nixosModules.default` with systemd template unit `mcserver@<instance>.service`.

---

## 🎮 TUI Controls & Keyboard Shortcuts

When launched without arguments (`mcserman`), the interactive TUI activates:

### Modo Navegación (`InputMode::Normal`)

| Tecla / Atajo | Acción |
|---|---|
| `j` / `Down` | Seleccionar siguiente servidor en la barra lateral |
| `k` / `Up` | Seleccionar servidor anterior en la barra lateral |
| `Tab` / `Shift+Tab` | Cambiar entre pestañas cíclicamente |
| `1` - `4` | Acceso directo a pestaña (`1`: Dashboard, `2`: Consola, `3`: Modrinth, `4`: Configuración) |
| `Space` | Iniciar / Detener servidor seleccionado (o alternar valores booleanos en Configuración) |
| `n` | Abrir asistente modal de creación de nueva instancia |
| `r` | Recargar lista de servidores y refrescar estado de sockets |
| `PgUp` / `PgDn` | Desplazarse por el historial de la consola |
| `i` / `Enter` | Entrar en modo escritura / foco en input de texto (`InputMode::Insert`) |
| `s` | Guardar cambios de `server.properties` a disco (pestaña Configuración) |
| `q` / `Ctrl+C` | Salir limpiamente de la TUI (restaura terminal alternate screen y raw mode) |

### Modo Edición / Inserción (`InputMode::Insert`)

Se activa al enfocar un campo de búsqueda (Modrinth), la línea de comandos (Consola) o el asistente modal de creación (`n`).

*   **Atajos globales desactivados**: Las teclas `q`, `j`, `k`, `Space`, `r` y dígitos `1-4` se comportan como caracteres normales de texto.
*   `Enter`: Enviar comando a la consola / Iniciar búsqueda en Modrinth / Confirmar creación en el Wizard.
*   `Esc`: Salir de modo edición y retornar inmediatamente a `InputMode::Normal`.

---

## 📥 Installation & Usage in NixOS

### 1. Ejecución Directa (Sin Instalación)

Prueba la TUI de inmediato en NixOS o cualquier distribución con Nix:
```bash
nix run github:AdrianChaSan/mcserman
```

### 2. Entorno de Desarrollo (`devShell`)

Accede a un entorno reproducible con Rust (`cargo`, `clippy`, `rustfmt`) y los 3 JDKs de Temurin listos:
```bash
git clone https://github.com/AdrianChaSan/mcserman.git
cd mcserman
nix develop
```

### 3. Módulo Declarativo en `configuration.nix`

Agrega `mcserman` a los inputs de tu `flake.nix`:

```nix
{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    mcserman.url = "github:AdrianChaSan/mcserman";
  };

  outputs = { self, nixpkgs, mcserman, ... }: {
    nixosConfigurations.my-host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./configuration.nix

        # 1. Importar el módulo de mcserman
        mcserman.nixosModules.default

        # 2. Configurar el servicio
        {
          services.mcserman = {
            enable = true;
            adminUser = "your_username"; # Permite gestionar instancias sin sudo
            openFirewall = {
              enable = true;
              port = 25565;
            };
            javaVersions = [ 21 17 8 ];  # JDKs Temurin preinstalados desde el Nix Store
          };
        }
      ];
    };
  };
}
```

---

## 🚀 Headless CLI Reference (Scripts & Systemd)

Los subcomandos CLI siguen estando 100% disponibles para automatización y scripts desatendidos:

```bash
# Crear instancia sin interacción
mcserman create survival --loader paper --mc-version 1.21.4 --ram 4G --port 25565

# Listar instancias registradas y estado online/offline
mcserman list

# Iniciar servidor en segundo plano
mcserman start survival

# Seguir logs en tiempo real (stdout)
mcserman logs survival -f

# Ejecutar comandos de consola
mcserman cmd survival "time set day"
mcserman cmd survival "say Servidor actualizándose..."

# Detener limpiamente (envía 'stop' a la JVM para guardar mundos)
mcserman stop survival

# Reiniciar instancia
mcserman restart survival

# Instalar mods o plugins desde Modrinth vía CLI
mcserman add-mod survival --query "voicechat"
```

---

## 🛠️ Architecture

```
                  ┌────────────────────────┐
                  │      Terminal TTY      │
                  └───────────┬────────────┘
                              │
               ┌──────────────┴──────────────┐
               │                             │
    (sin argumentos)                 (con subcomandos)
               ▼                             ▼
   ┌───────────────────────┐     ┌───────────────────────┐
   │      Ratatui TUI      │     │     Headless CLI      │
   │  - AppState           │     │  - clap subcommands   │
   │  - Crossterm EventLoop│     │  - scripts / systemd  │
   │  - TerminalGuard      │     └───────────┬───────────┘
   └───────────┬───────────┘                 │
               │                             │
               └──────────────┬──────────────┘
                              │
                              ▼
                 ┌─────────────────────────┐
                 │  spawn_daemon_detached  │
                 │   - POSIX setsid()      │
                 │   - Tokio Async Reaper  │
                 └────────────┬────────────┘
                              │
                              ▼
                 ┌─────────────────────────┐
                 │     mcserman daemon     │
                 │   - Unix Domain Socket  │
                 │   - JVM Child Process   │
                 │   - Auto SIGTERM 'stop' │
                 └─────────────────────────┘
```

---

## 📜 License

MIT License. Developed by [AdrianChaSan](https://github.com/AdrianChaSan).
