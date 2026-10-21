{ self }:
{ config, pkgs, lib, ... }:

with lib;

let
  cfg = config.services.mcserman;

  javaEnv = {
    MCSERVER_DATA_DIR = cfg.dataDir;
    MCSERVER_RUNTIME_DIR = "/run/mcserver";
  } // (optionalAttrs (builtins.elem 8 cfg.javaVersions) {
    MCSERVER_JAVA_8_PATH = "${pkgs.temurin-bin-8}/bin/java";
  }) // (optionalAttrs (builtins.elem 17 cfg.javaVersions) {
    MCSERVER_JAVA_17_PATH = "${pkgs.temurin-bin-17}/bin/java";
  }) // (optionalAttrs (builtins.elem 21 cfg.javaVersions) {
    MCSERVER_JAVA_21_PATH = "${pkgs.temurin-bin-21}/bin/java";
  }) // (optionalAttrs (builtins.elem 25 cfg.javaVersions) {
    MCSERVER_JAVA_25_PATH = "${pkgs.temurin-bin-25}/bin/java";
  });

in
{
  options.services.mcserman = {
    enable = mkEnableOption "Gestor autónomo de servidores Minecraft (mcserman)";

    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.system}.default;
      description = "Paquete de mcserman a utilizar.";
    };

    adminUser = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Usuario del sistema con permisos para gestionar instancias sin sudo (opcional).";
    };

    dataDir = mkOption {
      type = types.str;
      default = "/var/lib/mcserver";
      description = "Directorio base de almacenamiento de instancias y mundos.";
    };

    javaVersions = mkOption {
      type = types.listOf (types.enum [ 8 17 21 25 ]);
      default = [ 8 17 21 25 ];
      description = "Versiones del JDK Eclipse Temurin a preinstalar desde el Nix Store.";
    };

    openFirewall = {
      enable = mkOption {
        type = types.bool;
        default = true;
        description = "Abrir el puerto TCP del servidor de Minecraft.";
      };
      port = mkOption {
        type = types.port;
        default = 25565;
        description = "Puerto TCP del servidor.";
      };
      interface = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = "Interfaz de red en la cual abrir el puerto (null para todas las interfaces, o 'tailscale0', 'eth0', etc.).";
      };
    };
  };

  config = mkIf cfg.enable {
    # 1. Usuario y grupo del sistema
    users.users = {
      mcserver = {
        isSystemUser = true;
        home = cfg.dataDir;
        createHome = true;
        group = "mcserver";
        shell = pkgs.bash;
        description = "Minecraft Dedicated Server User";
      };
    } // (optionalAttrs (cfg.adminUser != null) {
      ${cfg.adminUser}.extraGroups = [ "mcserver" ];
    });
    users.groups.mcserver = {};

    # 2. Permisos con bit setgid (2775) para que los archivos creados pertenezcan al grupo mcserver
    systemd.tmpfiles.rules = [
      "d ${cfg.dataDir} 2775 mcserver mcserver -"
      "d ${cfg.dataDir}/instances 2775 mcserver mcserver -"
      "d ${cfg.dataDir}/runtimes 2775 mcserver mcserver -"
      "d /run/mcserver 0775 mcserver mcserver -"
    ];

    # 3. Exponer el binario en el PATH del sistema
    environment.systemPackages = [ cfg.package ];

    # Variables de entorno globales para que el CLI sepa las rutas
    environment.sessionVariables = javaEnv;

    # 4. Plantilla de servicio Systemd Template Unit: mcserver@.service
    systemd.services."mcserver@" = {
      description = "Minecraft Dedicated Server Instance: %i";
      wantedBy = [ ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];

      environment = javaEnv;

      serviceConfig = {
        User = "mcserver";
        Group = "mcserver";
        UMask = "0002";
        WorkingDirectory = "${cfg.dataDir}/instances/%i";
        RuntimeDirectory = "mcserver";
        RuntimeDirectoryMode = "0775";

        Restart = "on-failure";
        RestartSec = "15s";
        RestartPreventExitStatus = "143";
        TimeoutStartSec = "300";
        TimeoutStopSec = "60";

        # Sandboxing estricto
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        CapabilityBoundingSet = "";
        ReadWritePaths = [ cfg.dataDir ];

        ExecStart = "${cfg.package}/bin/mcserman daemon %i";
      };
    };

    # 5. Configuración del Firewall
    networking.firewall = mkIf cfg.openFirewall.enable (
      if cfg.openFirewall.interface == null then {
        allowedTCPPorts = [ cfg.openFirewall.port ];
      } else {
        interfaces."${cfg.openFirewall.interface}".allowedTCPPorts = [ cfg.openFirewall.port ];
      }
    );

    # 6. Reglas de sudo para que adminUser gestione instancias sin contraseña
    security.sudo.extraRules = mkIf (cfg.adminUser != null) [
      {
        users = [ cfg.adminUser ];
        commands = [
          { command = "/run/current-system/sw/bin/systemctl start mcserver@*"; options = [ "NOPASSWD" ]; }
          { command = "/run/current-system/sw/bin/systemctl stop mcserver@*"; options = [ "NOPASSWD" ]; }
          { command = "/run/current-system/sw/bin/systemctl restart mcserver@*"; options = [ "NOPASSWD" ]; }
          { command = "/run/current-system/sw/bin/systemctl status mcserver@*"; options = [ "NOPASSWD" ]; }
        ];
      }
    ];
  };
}
