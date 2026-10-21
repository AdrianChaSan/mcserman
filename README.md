# ⛏️ mcserman

A terminal Minecraft server manager built with Rust and [Ratatui](https://github.com/ratatui/ratatui), tailored for NixOS and Linux.
It provides an intuitive, keyboard-driven dashboard to create, manage, configure, and monitor multi-instance Minecraft servers with zero bloat.

---

## ✨ Features

- **Multi-Instance Management**: Seamlessly create and manage Paper, Fabric, and Mojang Vanilla server instances with automated loader provisioning and multi-JDK support (Java 21, 17, 8).
- **Integrated Live Console**: Real-time server log streaming with ANSI color rendering, auto-scrolling, and interactive in-TUI command execution.
- **Modrinth Browser & Installer**: Search, inspect, and install compatible mods and plugins directly inside the TUI with SHA-512 integrity verification.
- **Visual `server.properties` Editor**: Interactive key-value configuration editor to inspect and edit server properties with instant disk persistence.
- **Declarative NixOS Integration**: Native NixOS module and Flake packaging with pre-wrapped Eclipse Temurin Java runtimes from the Nix store.

---

## 📦 Installation

### NixOS / Flakes

Run directly in any Nix-enabled environment without installing:

```bash
nix run github:AdrianChaSan/mcserman
```

Or add it declaratively to your `flake.nix`:

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
        mcserman.nixosModules.default
        {
          services.mcserman = {
            enable = true;
            adminUser = "your_username"; # Manage instances without sudo
            openFirewall = {
              enable = true;
              port = 25565;
            };
            javaVersions = [ 21 17 8 ];  # Bundled Temurin JDKs from Nix store
          };
        }
      ];
    };
  };
}
```

### Cargo

Install from source using Cargo:

```bash
cargo install --git https://github.com/AdrianChaSan/mcserman
```

---

## 🎮 Usage

Simply run `mcserman` in your terminal to launch the interactive TUI dashboard:

```bash
mcserman
```

### Keybindings

| Key | Action |
|---|---|
| `j` / `Down` | Select next server in sidebar |
| `k` / `Up` | Select previous server in sidebar |
| `Tab` / `Shift+Tab` | Cycle between tabs |
| `1` - `4` | Direct tab switch (`1`: Dashboard, `2`: Console, `3`: Modrinth, `4`: Config) |
| `Space` | Start / Stop selected server (or toggle booleans in Config) |
| `n` | Open modal wizard to create a new instance |
| `r` | Refresh instance statuses and reload sockets |
| `s` | Save `server.properties` modifications to disk |
| `PgUp` / `PgDn` | Scroll console log history |
| `i` / `Enter` | Enter text input mode (`InputMode::Insert`) |
| `Esc` | Return to navigation mode (`InputMode::Normal`) |
| `q` / `Ctrl+C` | Cleanly exit `mcserman` |

---

## 📜 License

MIT License. Developed by [AdrianChaSan](https://github.com/AdrianChaSan).
