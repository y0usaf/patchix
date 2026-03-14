{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.patchix;
  patchix = pkgs.callPackage ./package.nix {};
  inherit
    (lib)
    filterAttrs
    mkEnableOption
    mkOption
    literalExpression
    mkIf
    mapAttrsToList
    mapAttrs'
    nameValuePair
    concatStringsSep
    concatMapStringsSep
    escapeShellArg
    optionalString
    getExe
    hasInfix
    concatLists
    ;
  inherit
    (lib.types)
    attrsOf
    submodule
    enum
    bool
    attrs
    ;

  # Generate the patch file in the Nix store
  mkPatchFile = name: patchCfg: let
    ext = patchCfg.format;
    content =
      if patchCfg.format == "json"
      then builtins.toJSON patchCfg.value
      else if patchCfg.format == "toml"
      then (pkgs.formats.toml {}).generate "patch-${name}" patchCfg.value
      else if patchCfg.format == "yaml"
      then (pkgs.formats.yaml {}).generate "patch-${name}" patchCfg.value
      else if patchCfg.format == "ini"
      then (pkgs.formats.ini {}).generate "patch-${name}" patchCfg.value
      else builtins.throw "Unsupported patch format: ${patchCfg.format}";
  in
    if builtins.isString content
    then pkgs.writeText "patchix-${name}.${ext}" content
    else content;

  # Generate a patchix CLI invocation for a single patch
  mkPatchInvocation = homeDir: target: patchCfg: let
    patchFile = mkPatchFile target patchCfg;
    fullTarget = "${homeDir}/${target}";
    strategyArgs = concatMapStringsSep " " (
      path: "--array-strategy ${escapeShellArg "${path}=${patchCfg.arrayStrategies.${path}}"}"
    ) (builtins.attrNames patchCfg.arrayStrategies);
  in ''
    ${getExe patchix} merge \
      --existing "${fullTarget}" \
      --patch "${patchFile}" \
      --format ${escapeShellArg patchCfg.format} \
      --default-array ${escapeShellArg patchCfg.defaultArrayStrategy} \
      ${optionalString (!patchCfg.clobber) "--no-clobber"} \
      ${strategyArgs}
  '';

  # Generate the full activation script for a user
  mkActivationScript = username: userCfg: let
    homeDir = config.users.users.${username}.home;
    enabledPatches = filterAttrs (_: p: p.enable) userCfg.patches;
    invocations = mapAttrsToList (mkPatchInvocation homeDir) enabledPatches;
  in
    pkgs.writeShellScript "patchix-activate-${username}" (concatStringsSep "\n" invocations);

  enabledUsers = filterAttrs (_: u: u.patches != {}) cfg.users;
in {
  options.patchix = {
    enable = mkEnableOption "patchix declarative config patching";

    users = mkOption {
      type = attrsOf (submodule {
        options = {
          patches = mkOption {
            type = attrsOf (submodule {
              options = {
                enable = mkOption {
                  type = bool;
                  default = true;
                  description = "Whether to enable this patch.";
                };
                clobber = mkOption {
                  type = bool;
                  default = true;
                  description = ''
                    Whether to overwrite existing values.
                    When false, only missing keys are filled in — runtime changes are preserved.
                  '';
                };
                format = mkOption {
                  type = enum [
                    "json"
                    "toml"
                    "yaml"
                    "ini"
                  ];
                  description = "Config file format.";
                };
                value = mkOption {
                  type = attrs;
                  default = {};
                  description = "Patch content as a Nix attribute set. Deep-merged into the existing file.";
                };
                defaultArrayStrategy = mkOption {
                  type = enum [
                    "replace"
                    "append"
                    "prepend"
                    "union"
                  ];
                  default = "replace";
                  description = "Default strategy for merging arrays.";
                };

                arrayStrategies = mkOption {
                  type = attrsOf (enum [
                    "replace"
                    "append"
                    "prepend"
                    "union"
                  ]);
                  default = {};
                  description = "Per-path array merge strategy overrides. Keys are dot-separated paths.";
                  example = {
                    "plugins" = "append";
                    "editor.formatters" = "union";
                  };
                };
              };
            });
            default = {};
            description = "Config files to patch. Keys are target paths relative to the user's home directory.";
          };
        };
      });
      default = {};
      description = "Per-user config file patches.";
      example = literalExpression ''
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

  config = mkIf cfg.enable {
    assertions = concatLists (
      mapAttrsToList (username: userCfg:
        [
          {
            assertion = config.users.users ? ${username};
            message = "patchix: user '${username}' in patchix.users is not defined in users.users";
          }
        ]
        ++ mapAttrsToList (target: _: {
          assertion = !(hasInfix ".." target);
          message = "patchix: patch target '${target}' for user '${username}' contains '..' path traversal";
        })
        userCfg.patches)
      enabledUsers
    );

    environment.systemPackages = [patchix];

    systemd.services = mkIf (enabledUsers != {}) (
      mapAttrs' (username: userCfg:
        nameValuePair "patchix-${username}" {
          description = "patchix: apply config patches for ${username}";
          wantedBy = ["multi-user.target"];
          after = ["multi-user.target"];
          serviceConfig = {
            Type = "oneshot";
            User = username;
            ExecStart = mkActivationScript username userCfg;
            RemainAfterExit = true;
          };
        })
      enabledUsers
    );
  };
}
