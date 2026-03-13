{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.patchix;
  patchixPkg = pkgs.rustPlatform.buildRustPackage {
    pname = "patchix";
    version = "0.1.0";
    src = lib.cleanSource ../.;
    cargoLock.lockFile = ../Cargo.lock;
    meta.mainProgram = "patchix";
  };

  # Generate INI content from a nested attrset:
  # { "__global__" = { key = val; }; section = { key = val; }; }

  # Generate the patch file in the Nix store

  # Generate a patchix CLI invocation for a single patch

  # Generate the full activation script for a user

  enabledUsers = lib.filterAttrs (_: u: u.patches != {}) cfg.users;
in {
  options.patchix = {
    enable = lib.mkEnableOption "patchix declarative config patching";

    users = lib.mkOption {
      type = lib.types.attrsOf (lib.types.submodule ({name, ...}: {
    options = {
      patches = lib.mkOption {
        type = lib.types.attrsOf (lib.types.submodule ({name, ...}: {
    options = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to enable this patch.";
      };

      clobber = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = ''
          Whether to overwrite existing values.
          When false, only missing keys are filled in — runtime changes are preserved.
        '';
      };

      format = lib.mkOption {
        type = lib.types.enum ["json" "toml" "yaml" "ini"];
        description = "Config file format.";
      };

      value = lib.mkOption {
        type = lib.types.attrs;
        default = {};
        description = "Patch content as a Nix attribute set. Deep-merged into the existing file.";
      };

      defaultArrayStrategy = lib.mkOption {
        type = lib.types.enum ["replace" "append" "prepend" "union"];
        default = "replace";
        description = "Default strategy for merging arrays.";
      };

      arrayStrategies = lib.mkOption {
        type = lib.types.attrsOf (lib.types.enum ["replace" "append" "prepend" "union"]);
        default = {};
        description = "Per-path array merge strategy overrides. Keys are dot-separated paths.";
        example = {
          "plugins" = "append";
          "editor.formatters" = "union";
        };
      };
    };
  }));
        default = {};
        description = "Config files to patch. Keys are target paths relative to the user's home directory.";
      };
    };
  }));
      default = {};
      description = "Per-user config file patches.";
      example = lib.literalExpression ''
        {
          y0usaf.patches = {
            ".config/Code/User/settings.json" = {
              format = "json";
              value = {
                "editor.fontSize" = 14;
                "workbench.colorTheme" = "One Dark Pro";
              };
            };
          };
        }
      '';
    };
  };

  config = lib.mkIf (cfg.enable && enabledUsers != {}) {
    assertions =
      lib.concatLists (lib.mapAttrsToList (username: userCfg:
        [{
          assertion = config.users.users ? ${username};
          message = "patchix: user '${username}' in patchix.users is not defined in users.users";
        }]
        ++ lib.mapAttrsToList (target: _: {
          assertion = !(lib.hasInfix ".." target);
          message = "patchix: patch target '${target}' for user '${username}' contains '..' path traversal";
        }) userCfg.patches
      ) enabledUsers);

    # Add patchix to system packages so it's available
    environment.systemPackages = [patchixPkg];

    # Per-user systemd oneshot services
    systemd.services = lib.mapAttrs' (username: userCfg:
      lib.nameValuePair "patchix-${username}" {
        description = "patchix: apply config patches for ${username}";
        wantedBy = ["multi-user.target"];
        after = ["multi-user.target"];
        serviceConfig = {
          Type = "oneshot";
          User = username;
          ExecStart = (username: userCfg: pkgs.writeShellScript "patchix-activate-${username}" (
      lib.concatStringsSep "\n" (
        lib.mapAttrsToList ((homeDir: target: patchCfg: let
    patchFile = (name: patchCfg: let
    ext = {
    json = "json";
    toml = "toml";
    yaml = "yaml";
    ini = "ini";
  }."${patchCfg.format}";
    content =
      if patchCfg.format == "json"
      then builtins.toJSON patchCfg.value
      else if patchCfg.format == "toml"
      then (pkgs.formats.toml {}).generate "patch-${name}" patchCfg.value
      else if patchCfg.format == "yaml"
      then (pkgs.formats.yaml {}).generate "patch-${name}" patchCfg.value
      else (attrs: let
  formatIniVal = v:
    if builtins.isBool v then lib.boolToString v
    else if builtins.isInt v || builtins.isFloat v then toString v
    else if builtins.isString v then v
    else throw "patchix: INI values must be scalars, got ${builtins.typeOf v}";
  renderSection = section: props: let
    header = if section == "__global__" then "" else "[${section}]\n";
    lines = lib.concatStringsSep "\n" (lib.mapAttrsToList (k: v: "${k} = ${formatIniVal v}") props);
  in "${header}${lines}";
in lib.concatStringsSep "\n" (lib.mapAttrsToList renderSection attrs) + "\n") patchCfg.value;
  in
    if builtins.isString content
    then pkgs.writeText "patchix-${name}.${ext}" content
    else content) target patchCfg;
    fullTarget = "${homeDir}/${target}";
    strategyArgs = lib.concatMapStringsSep " "
      (path: "--array-strategy ${lib.escapeShellArg "${path}=${patchCfg.arrayStrategies."${path}"}"}")
      (builtins.attrNames patchCfg.arrayStrategies);
  in ''
    ${lib.getExe patchixPkg} merge \
      --existing "${fullTarget}" \
      --patch "${patchFile}" \
      --format ${lib.escapeShellArg patchCfg.format} \
      --default-array ${lib.escapeShellArg patchCfg.defaultArrayStrategy} \
      ${lib.optionalString (!patchCfg.clobber) "--no-clobber"} \
      ${strategyArgs}
  '') config.users.users."${username}".home) (lib.filterAttrs (_: p: p.enable) userCfg.patches)
      )
    )) username userCfg;
          RemainAfterExit = true;
        };
      }
    ) enabledUsers;
  };
}
