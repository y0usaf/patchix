{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.patchix;
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
    concatLists
    hasPrefix
    splitString
    ;
  inherit
    (lib.types)
    attrsOf
    submodule
    enum
    bool
    attrs
    package
    ;

  # Generate the patch file in the Nix store. Registry patches are emitted as
  # JSON and parsed with --patch-format json because expressing .reg directly in
  # Nix is awkward and lossy.
  mkPatchFile = name: patchCfg: let
    patchFormat =
      if patchCfg.format == "reg"
      then "json"
      else patchCfg.format;
    content =
      if patchFormat == "json"
      then builtins.toJSON patchCfg.value
      else if patchFormat == "toml"
      then (pkgs.formats.toml {}).generate "patch-${name}" patchCfg.value
      else if patchFormat == "yaml"
      then (pkgs.formats.yaml {}).generate "patch-${name}" patchCfg.value
      else if patchFormat == "ini"
      then
        lib.generators.toINIWithGlobalSection {} {
          # map __global__
          globalSection = patchCfg.value.__global__ or {};
          # everything else
          sections = builtins.removeAttrs patchCfg.value ["__global__"];
        }
      else builtins.throw "Unsupported patch format: ${patchCfg.format}";
  in {
    format = patchFormat;
    path =
      if builtins.isString content
      then pkgs.writeText "patchix-${name}.${patchFormat}" content
      else content;
  };

  # Generate a patchix CLI invocation for a single patch
  mkPatchInvocation = homeDir: target: patchCfg: let
    patchSpec = mkPatchFile target patchCfg;
    fullTarget = "${homeDir}/${target}";
    strategyArgs = concatMapStringsSep " " (
      path: "--array-strategy ${escapeShellArg "${path}=${patchCfg.arrayStrategies.${path}}"}"
    ) (builtins.attrNames patchCfg.arrayStrategies);
  in ''
    ${getExe cfg.package} merge \
      --existing ${escapeShellArg fullTarget} \
      --patch ${escapeShellArg (toString patchSpec.path)} \
      --format ${escapeShellArg patchCfg.format} \
      --patch-format ${escapeShellArg patchSpec.format} \
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

  # Fix 3: only include users who have at least one enabled patch
  enabledUsers = filterAttrs (
    _: u: builtins.any (p: p.enable) (builtins.attrValues u.patches)
  ) cfg.users;
in {
  options.patchix = {
    enable = mkEnableOption "patchix declarative config patching";

    # Fix 4: package option for binary override
    package = mkOption {
      type = package;
      default = pkgs.callPackage ./package.nix {};
      defaultText = literalExpression "pkgs.callPackage ./package.nix {}";
      description = "The patchix package to use.";
    };

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
                    "reg"
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
    # Fix 1: improved path traversal assertions — absolute path check and
    # component-level ".." check, plus empty-string guard.
    assertions =
      # Validate user existence for ALL configured users (even those with all patches disabled)
      (mapAttrsToList (username: _: {
        assertion = config.users.users ? ${username};
        message = "patchix: user '${username}' in patchix.users is not defined in users.users";
      }) cfg.users)
      ++
      # Validate target paths only for users with enabled patches
      concatLists (
        mapAttrsToList (username: userCfg:
          concatLists (mapAttrsToList (target: _: [
            {
              assertion = !(hasPrefix "/" target);
              message = "patchix: patch target '${target}' for user '${username}' must be a relative path (no leading '/')";
            }
            {
              assertion = !(builtins.any (c: c == "..") (splitString "/" target));
              message = "patchix: patch target '${target}' for user '${username}' contains '..' path traversal";
            }
            {
              assertion = target != "";
              message = "patchix: patch target for user '${username}' must not be an empty string";
            }
          ]) userCfg.patches)
        ) enabledUsers
      );

    # Fix 4: use cfg.package instead of the removed local patchix binding
    environment.systemPackages = [cfg.package];

    systemd.services = mkIf (enabledUsers != {}) (
      mapAttrs' (username: userCfg:
        nameValuePair "patchix-${username}" {
          description = "patchix: apply config patches for ${username}";
          wantedBy = ["multi-user.target"];
          after = ["multi-user.target"];
          unitConfig.RequiresMountsFor = config.users.users.${username}.home;
          serviceConfig = let
            homeDir = config.users.users.${username}.home;
          in {
            Type = "oneshot";
            User = username;
            ExecStart = mkActivationScript username userCfg;

            # RemainAfterExit = true keeps the unit marked "active (exited)" after
            # a successful run; it re-executes on every boot when multi-user.target
            # is reached. To manually re-apply mid-session:
            #   sudo systemctl restart patchix-<username>.service
            RemainAfterExit = true;

            # Fix 5: security hardening — patchix only needs to read from the Nix
            # store and write to the target user's home directory.
            NoNewPrivileges = true;
            ProtectSystem = "strict";
            ProtectHome = "read-only";
            ReadWritePaths = [homeDir];
            PrivateTmp = true;
            PrivateDevices = true;
            ProtectKernelTunables = true;
            ProtectControlGroups = true;
            RestrictNamespaces = true;
            LockPersonality = true;
            RestrictRealtime = true;
            SystemCallFilter = ["@system-service" "~@privileged" "~@resources"];
          };
        })
      enabledUsers
    );
  };
}
