# Ironclad Init System and Service Syntax Specification

**Status:** Draft — syntax development, Phase 1  
**Scope:** Init system selection, service declarations, dependency ordering, socket activation, timers, targets/bundles, logging, readiness, resource controls, and the interface between service declarations and SELinux/filesystem/storage/bootloader specifications  
**Dependencies:** `LANGUAGE_SYNTAX.md`, `FILESYSTEM_SYNTAX.md`, `SELINUX_SYNTAX.md`, `BOOTLOADER_SYNTAX.md`

---

## Design Principles

An init system supervises processes. It starts them, restarts them when they die, orders them relative to each other, and provides mechanisms for readiness notification, socket handoff, and logging. Ironclad declares what the init system should do. The compiler emits the backend-specific artifacts — systemd unit files, s6 service directories, or s6-rc service definitions — and the runtime agent monitors for drift against those artifacts.

The core constraints:

1. **The init system is declared, not implied.** Every Ironclad system explicitly declares which init system it uses. There is no default. The compiler errors if no init block is present. This forces the operator to make the choice consciously.

2. **Services are close to the metal.** Ironclad does not invent an abstract service model that papers over the differences between systemd and s6. A systemd service has `ExecStart`, `Type`, `After`, and `WantedBy`. An s6 service has a `run` script, a `finish` script, a `type` declaration, and dependency edges in s6-rc. The syntax for each backend reflects the backend's real semantics. Operators who know their init system will recognize the declarations immediately.

3. **Common properties are shared.** Where both backends genuinely share a concept — a service has a name, runs as a user, has an SELinux label, restarts on failure — the property name and semantics are identical. The compiler maps them to the backend's representation. This is not abstraction for portability; it is consistency for readability.

4. **Cross-references are validated.** A service that runs as user `postgres` is validated against the system's user declarations. A service with SELinux type `httpd_t` is validated against the SELinux policy declaration. A service that binds port 443 is cross-referenced against the firewall declaration. A service whose `ExecStart` references `/usr/sbin/httpd` is validated against the filesystem tree.

5. **Logging is not optional.** Every long-running service must have a declared logging strategy. The compiler errors if a service has no logging declaration and the security floor is `standard` or above. s6 and systemd handle logging differently — s6 pairs each service with a dedicated log service via a pipe; systemd routes stdout/stderr to journald. The syntax reflects this difference.

6. **The init block owns `/etc/systemd/` and `/etc/s6/` and their contents.** Service declarations compile to files in these directories. The filesystem syntax (`FILESYSTEM_SYNTAX.md`) can declare additional files alongside them, but the service declarations are the primary authors. Conflicts between a service declaration and an inline file declaration follow the standard merge rules: inline wins, warning emitted.

---

## System-Level Init Block

Every Ironclad system contains exactly one `init` block. The keyword after `init` is the **backend**.

```
system web01 {
    init systemd {
        # system-level systemd configuration and service declarations
    }
}
```

```
system container_host {
    init s6 {
        # system-level s6/s6-rc configuration and service declarations
    }
}
```

### Backend Keywords

| Keyword    | Init System        | Notes                                                           |
|------------|--------------------|-----------------------------------------------------------------|
| `systemd`  | systemd            | Default on RHEL, Fedora, AlmaLinux. Unit files, targets, timers, socket activation. |
| `s6`       | s6 + s6-rc         | Lightweight, explicit supervision. s6-rc for dependency management, s6-log for logging. |
| `raw`      | None               | Compiler emits no init configuration. Operator provides everything via `FILEOPS_SYNTAX.md` raw mechanisms. For minimal containers or custom init. |

The compiler validates backend compatibility with the base image. An `init s6` declaration on a system whose package list does not include `s6` is a compile error. An `init systemd` declaration is validated against the presence of the systemd package set.

---

## Init Block Properties

Properties declared directly inside the `init` block configure system-wide init behavior.

### systemd

```
init systemd {
    default_target = "multi-user.target"

    journal {
        storage = persistent           # volatile | persistent | auto | none
        max_size = 500M
        max_retention = "1month"
        compress = true
        seal = true                    # FSS sealing for tamper evidence
        forward_to_syslog = false
    }

    coredump {
        storage = journal              # none | external | journal
        max_size = 2G
        process_limit = 16
    }

    # System-wide resource defaults
    defaults {
        restart_policy = on-failure
        restart_delay = 5s
        timeout_start = 90s
        timeout_stop = 90s
        oom_policy = stop              # stop | continue | kill
    }
}
```

| Property          | Type      | Default                | Description                                       |
|-------------------|-----------|------------------------|---------------------------------------------------|
| `default_target`  | `string`  | `"multi-user.target"`  | The default systemd target. Reference to a declared or well-known target. |

The `journal` sub-block configures `/etc/systemd/journald.conf`. The `coredump` sub-block configures `/etc/systemd/coredump.conf`. Both are optional — when omitted, the package defaults apply.

The `defaults` sub-block sets properties inherited by all service declarations that do not override them. This avoids repeating `restart_policy = on-failure` in every service.

### s6

```
init s6 {
    scan_dir = /run/service            # s6-svscan scan directory
    log_dir = /var/log/s6              # base directory for s6-log output
    catch_all_log = true               # run a catch-all logger for unlogged services
    timestamp_format = tai64n          # tai64n | iso8601 | none

    rc {
        compiled_dir = /etc/s6-rc/compiled
        source_dir = /etc/s6-rc/source
        timeout_up = 30000             # milliseconds
        timeout_down = 30000
    }

    defaults {
        restart_policy = on-failure
        restart_delay = 1s
        max_restart_count = 50
        max_restart_period = 10s
    }
}
```

| Property            | Type      | Default           | Description                                          |
|---------------------|-----------|-------------------|------------------------------------------------------|
| `scan_dir`          | `path`    | `/run/service`    | s6-svscan's scan directory.                          |
| `log_dir`           | `path`    | `/var/log/s6`     | Base directory for s6-log service output.            |
| `catch_all_log`     | `bool`    | `true`            | Whether to create a catch-all logger for stdout/stderr not captured by per-service loggers. |
| `timestamp_format`  | `string`  | `tai64n`          | Timestamp format for s6-log. `tai64n` is s6's native format. `iso8601` uses `s6-tai64nlocal` for human-readable output. |

The `rc` sub-block configures s6-rc compilation and execution parameters.

---

## Service Declarations

Services are declared as named blocks inside the `init` block. The syntax varies by backend but shares common property names where semantics align.

### systemd Services

```
init systemd {
    service httpd {
        description = "Apache HTTP Server"
        type = notify                  # simple | exec | forking | oneshot | notify | dbus | idle
        exec_start = "/usr/sbin/httpd -DFOREGROUND"
        exec_reload = "/bin/kill -USR1 $MAINPID"

        user = apache
        group = apache
        selinux = httpd_t

        after = [network-online.target, sshd.service]
        wants = [network-online.target]
        wanted_by = [multi-user.target]
        requires = []
        conflicts = [nginx.service]

        restart = on-failure
        restart_delay = 5s
        timeout_start = 30s
        timeout_stop = 30s

        environment = {
            LANG = "C.UTF-8"
            OPTIONS = "-DFOREGROUND"
        }

        environment_file = /etc/sysconfig/httpd

        resource_control {
            memory_max = 2G
            memory_high = 1500M
            cpu_weight = 100
            tasks_max = 4096
            io_weight = 100
        }

        logging {
            # systemd services log to journal by default.
            # This block configures additional behavior.
            stdout = journal             # journal | null | file:<path> | socket:<path>
            stderr = journal
            syslog_identifier = "httpd"
            syslog_facility = daemon
        }

        hardening {
            protect_system = strict      # true | full | strict
            protect_home = true
            private_tmp = true
            private_devices = true
            no_new_privileges = true
            protect_kernel_tunables = true
            protect_kernel_modules = true
            protect_control_groups = true
            restrict_suid_sgid = true
            lock_personality = true
            memory_deny_write_execute = true
            restrict_namespaces = true
            restrict_realtime = true
            system_call_filter = ["@system-service", "~@privileged", "~@resources"]
            capability_bounding_set = [CAP_NET_BIND_SERVICE]
        }

        install {
            wanted_by = [multi-user.target]
            also = [httpd.socket]
        }
    }
}
```

### systemd Service Properties

#### Core Properties

| Property        | Type          | Default       | Description                                                          |
|-----------------|---------------|---------------|----------------------------------------------------------------------|
| `description`   | `string`      | (none)        | Human-readable description. Maps to `Description=`.                  |
| `type`          | enum          | `simple`      | Service type: `simple`, `exec`, `forking`, `oneshot`, `notify`, `dbus`, `idle`. |
| `exec_start`    | `string`      | (required)    | The command to start the service. Maps to `ExecStart=`.              |
| `exec_stop`     | `string`      | (none)        | Custom stop command. Maps to `ExecStop=`.                            |
| `exec_reload`   | `string`      | (none)        | Reload command. Maps to `ExecReload=`.                               |
| `exec_start_pre`| `list[string]`| `[]`          | Commands run before `exec_start`. Maps to `ExecStartPre=`.          |
| `exec_start_post`| `list[string]`| `[]`         | Commands run after `exec_start`. Maps to `ExecStartPost=`.          |
| `remain_after_exit` | `bool`    | `false`       | For `oneshot`: keep service active after process exits.              |
| `pid_file`      | `path`        | (none)        | PID file path for `forking` type services. Maps to `PIDFile=`.      |
| `bus_name`      | `string`      | (none)        | D-Bus name for `dbus` type services. Maps to `BusName=`.            |
| `notify_access`  | enum         | `main`        | Who can send readiness notifications: `none`, `main`, `exec`, `all`. |

#### Identity Properties

| Property   | Type     | Default  | Description                                                                |
|------------|----------|----------|----------------------------------------------------------------------------|
| `user`     | `string` | `root`   | User to run the service as. Validated against user declarations.           |
| `group`    | `string` | (none)   | Primary group. Validated against group declarations.                       |
| `selinux`  | `string` | (none)   | SELinux process type. Validated against the SELinux policy declaration.     |

#### Dependency Properties

| Property     | Type           | Default | Description                                                     |
|--------------|----------------|---------|------------------------------------------------------------------|
| `after`      | `list[string]` | `[]`    | Start after these units. Maps to `After=`.                       |
| `before`     | `list[string]` | `[]`    | Start before these units. Maps to `Before=`.                     |
| `requires`   | `list[string]` | `[]`    | Hard dependency. Maps to `Requires=`.                            |
| `wants`      | `list[string]` | `[]`    | Soft dependency. Maps to `Wants=`.                               |
| `conflicts`  | `list[string]` | `[]`    | Mutual exclusion. Maps to `Conflicts=`.                          |
| `wanted_by`  | `list[string]` | `[]`    | Install section targets. Maps to `[Install] WantedBy=`.          |
| `required_by`| `list[string]` | `[]`    | Install section hard dependency. Maps to `[Install] RequiredBy=`.|

The compiler validates that dependency targets either reference other declared services (by name, with `.service` suffix inferred), declared targets, or well-known systemd units (the compiler ships a list of standard systemd units that are assumed to exist on RHEL-family systems).

#### Restart Properties

| Property          | Type     | Default        | Description                                                |
|-------------------|----------|----------------|------------------------------------------------------------|
| `restart`         | enum     | `on-failure`   | `no`, `always`, `on-success`, `on-failure`, `on-abnormal`, `on-abort`, `on-watchdog`. |
| `restart_delay`   | duration | `100ms`        | Time between restart attempts. Maps to `RestartSec=`.      |
| `restart_max`     | `int`    | (none)         | Maximum restart count in `restart_window`. Maps to `StartLimitBurst=`. |
| `restart_window`  | duration | (none)         | Time window for `restart_max`. Maps to `StartLimitIntervalSec=`. |
| `timeout_start`   | duration | `90s`          | Time to wait for startup. Maps to `TimeoutStartSec=`.      |
| `timeout_stop`    | duration | `90s`          | Time to wait for graceful stop. Maps to `TimeoutStopSec=`. |

#### Environment

| Property           | Type              | Default | Description                                             |
|--------------------|-------------------|---------|---------------------------------------------------------|
| `environment`      | `map[string, string]` | `{}`  | Key-value environment variables. Maps to `Environment=`. |
| `environment_file` | `path` or `list[path]` | (none) | Environment file(s) loaded at start. Maps to `EnvironmentFile=`. Validated against the filesystem tree. |

---

### s6 Services

```
init s6 {
    service httpd {
        description = "Apache HTTP Server"
        type = longrun                 # longrun | oneshot | bundle

        run = """
            #!/bin/execlineb -P
            fdmove -c 2 1
            s6-setuidgid apache
            /usr/sbin/httpd -DFOREGROUND
        """

        finish = """
            #!/bin/execlineb -S0
            foreground { s6-echo "httpd exited with code ${1}" }
            if { s6-test ${1} -ne 0 }
            s6-sleep 1000
        """

        user = apache
        group = apache
        selinux = httpd_t

        dependencies = [network, sshd]
        
        restart = on-failure
        restart_delay = 1s
        max_death_tally = 50
        min_death_tally_period = 10s
        timeout_up = 30000             # milliseconds
        timeout_down = 30000

        notification {
            fd = 3                     # readiness notification fd
            timeout = 30000            # milliseconds to wait for readiness
        }

        environment = {
            LANG = "C.UTF-8"
        }

        environment_dir = /etc/s6/env/httpd

        log {
            destination = /var/log/s6/httpd
            max_size = 10M
            max_files = 10
            processor = """
                #!/bin/execlineb -P
                s6-tai64nlocal
            """
            user = s6log
            group = s6log
            selinux = s6_log_t
        }
    }
}
```

### s6 Service Properties

#### Core Properties

| Property       | Type       | Default     | Description                                                          |
|----------------|------------|-------------|----------------------------------------------------------------------|
| `description`  | `string`   | (none)      | Human-readable description. Written to a `description` file in the service directory for tooling and documentation. |
| `type`         | enum       | `longrun`   | Service type: `longrun`, `oneshot`, `bundle`.                        |
| `run`          | `string`   | (required for longrun/oneshot) | The `run` script content. Written to the service's `run` file. Typically an execline script. |
| `finish`       | `string`   | (none)      | The `finish` script. Written to the service's `finish` file. Receives exit code as `$1` and wait status as `$2`. |
| `up`           | `string`   | (none)      | For oneshot: the transition-up script. Used by s6-rc instead of `run` for oneshot services. |
| `down`         | `string`   | (none)      | For oneshot: the transition-down script. The reverse operation. |
| `pipeline_name`| `string`   | (none)      | When this service is part of a logging pipeline, the pipeline group name. |

The compiler validates that `longrun` services have a `run` script, `oneshot` services have `up` (and optionally `down`), and `bundle` services have `contents` (see Bundles below).

#### Identity Properties

Same as systemd — `user`, `group`, `selinux`. The `user` and `group` values are used by the compiler to emit the appropriate `s6-setuidgid` or `s6-envuidgid` invocation in the `run` script preamble, or to set ownership on the service directory. The operator can handle user switching manually in the `run` script instead — if `user` is set to `root` (default), the compiler does not inject any uid/gid switching.

#### Dependency Properties

| Property        | Type           | Default | Description                                                    |
|-----------------|----------------|---------|----------------------------------------------------------------|
| `dependencies`  | `list[string]` | `[]`    | Services that must be up before this service starts. Maps to s6-rc dependency declarations. |
| `contents`      | `list[string]` | `[]`    | For `bundle` type: the services this bundle contains.          |

s6-rc dependencies are simpler than systemd's: a service depends on another, and that means it starts after the dependency is ready. There is no `wants` vs `requires` distinction — all dependencies are hard. If a dependency fails to start, the dependent does not start. This is an s6 design choice, not an Ironclad limitation.

The compiler validates that all dependency targets are declared services or bundles within the same init block.

#### Restart Properties

| Property                | Type     | Default        | Description                                                |
|-------------------------|----------|----------------|------------------------------------------------------------|
| `restart`               | enum     | `on-failure`   | `always`, `on-failure`, `never`. s6 always restarts by default; `on-failure` uses the `finish` script's exit code to decide. |
| `restart_delay`         | duration | `1s`           | Delay between restarts. Implemented via `s6-sleep` in the generated `finish` script if the operator's `finish` does not handle it. |
| `max_death_tally`       | `int`    | `50`           | Maximum number of deaths tracked. Maps to s6's `max-death-tally` flag. |
| `min_death_tally_period`| duration | `10s`          | Minimum time between deaths counted in the tally. |
| `timeout_up`            | `int`    | `30000`        | Milliseconds. Time to wait for readiness after starting. Maps to s6-rc `timeout-up`. |
| `timeout_down`          | `int`    | `30000`        | Milliseconds. Time to wait for service to stop. Maps to s6-rc `timeout-down`. |

#### Readiness Notification

| Property    | Type   | Default | Description                                                              |
|-------------|--------|---------|--------------------------------------------------------------------------|
| `notification.fd` | `int` | (none) | File descriptor the service writes to when ready. Maps to s6's notification-fd file. |
| `notification.timeout` | `int` | `30000` | Milliseconds. How long s6-supervise waits for readiness before declaring the service up anyway. |

When `notification.fd` is declared, the compiler writes the `notification-fd` file in the service directory. The service's `run` script is responsible for writing a newline to that fd when ready. This is how s6 achieves reliable dependency ordering — a dependent does not start until its dependency has signaled readiness.

#### Environment

| Property          | Type                    | Default | Description                                              |
|-------------------|-------------------------|---------|----------------------------------------------------------|
| `environment`     | `map[string, string]`   | `{}`    | Key-value pairs. Written as individual files in the service's `env` directory (s6-envdir format). |
| `environment_dir` | `path`                  | (none)  | Path to an existing envdir directory. The compiler validates the path exists in the filesystem tree. |

---

## Logging

Logging is handled differently between backends and the syntax reflects this.

### systemd Logging

systemd routes all service stdout/stderr to journald by default. The `logging` block in a systemd service configures additional behavior:

```
service myapp {
    logging {
        stdout = journal             # journal | null | file:<path> | socket:<path>
        stderr = journal
        syslog_identifier = "myapp"
        syslog_facility = daemon
        log_level_max = warning      # emerg | alert | crit | err | warning | notice | info | debug
    }
}
```

| Property             | Type     | Default     | Description                                               |
|----------------------|----------|-------------|-----------------------------------------------------------|
| `stdout`             | `string` | `journal`   | Where to send stdout. `journal` (default), `null`, `file:/path`, `socket:/path`. |
| `stderr`             | `string` | `journal`   | Where to send stderr. Same options as `stdout`.           |
| `syslog_identifier`  | `string` | service name| Syslog identifier. Maps to `SyslogIdentifier=`.          |
| `syslog_facility`    | `string` | `daemon`    | Syslog facility. Maps to `SyslogFacility=`.               |
| `log_level_max`      | enum     | (none)      | Maximum log level forwarded. Maps to `LogLevelMax=`.      |

For services that need file-based logging in addition to journal, `stdout = file:/var/log/myapp/output.log` causes the compiler to emit `StandardOutput=file:/var/log/myapp/output.log` and to validate the log path against the filesystem tree.

### s6 Logging

s6 pairs each service with a dedicated log service connected via a pipe. The `log` block inside an s6 service declares this logger:

```
service myapp {
    log {
        destination = /var/log/s6/myapp
        max_size = 10M               # per-file max before rotation
        max_files = 10               # number of rotated files to keep
        timestamp = tai64n           # tai64n | iso8601 | none
        processor = """
            #!/bin/execlineb -P
            s6-tai64nlocal
        """
        user = s6log
        group = s6log
        selinux = s6_log_t
    }
}
```

| Property       | Type      | Default             | Description                                                  |
|----------------|-----------|---------------------|--------------------------------------------------------------|
| `destination`  | `path`    | `${log_dir}/<name>` | Log output directory. The compiler creates this directory with the declared ownership and labels. |
| `max_size`     | `size`    | `1M`                | Maximum size of each log file before rotation.               |
| `max_files`    | `int`     | `10`                | Number of rotated log files to retain.                       |
| `timestamp`    | enum      | `tai64n`            | Timestamp format prepended to each line. Inherits from init-level `timestamp_format` if not set. |
| `processor`    | `string`  | (none)              | Log processor script. Runs on every line. Common use: `s6-tai64nlocal` for human-readable timestamps. |
| `user`         | `string`  | `s6log`             | User the log service runs as. Validated against user declarations. |
| `group`        | `string`  | `s6log`             | Group the log service runs as.                               |
| `selinux`      | `string`  | (none)              | SELinux type for the log service process.                    |

The compiler generates the companion log service directory automatically. The `pipeline_name` property on the parent service links the service and its logger into an s6 logging pipeline. If `log` is declared without `pipeline_name`, the compiler auto-generates the pipeline name as `<service_name>-log-pipeline`.

**Security floor enforcement:** At `standard` and above, every `longrun` service must have a `log` block. Services without a declared logger produce a compile error. At `baseline`, the compiler emits a warning.

---

## Socket Activation

### systemd Sockets

systemd socket activation is declared as a `socket` block inside the init block, separate from but referencing the service it activates:

```
init systemd {
    socket httpd {
        description = "Apache HTTP Server Socket"
        listen_stream = [80, 443]
        listen_stream = ["/run/httpd/httpd.sock"]
        bind_ipv6_only = both
        accept = false

        install {
            wanted_by = [sockets.target]
        }
    }

    service httpd {
        type = notify
        exec_start = "/usr/sbin/httpd -DFOREGROUND"
        # ...
    }
}
```

The compiler infers the socket-to-service relationship by matching names: `socket httpd` activates `service httpd`. An explicit `service` property inside the socket block overrides this:

```
socket httpd_alt {
    service = httpd              # activates service httpd, not httpd_alt
    listen_stream = [8080]
}
```

#### Socket Properties

| Property          | Type              | Default   | Description                                              |
|-------------------|-------------------|-----------|----------------------------------------------------------|
| `description`     | `string`          | (none)    | Human-readable description.                              |
| `listen_stream`   | `list[int\|path]` | (required)| TCP ports and/or Unix socket paths for stream sockets.   |
| `listen_datagram` | `list[int\|path]` | `[]`      | UDP ports and/or Unix socket paths for datagram sockets. |
| `listen_sequential_packet` | `list[path]` | `[]`  | Unix socket paths for sequential packet sockets.         |
| `listen_fifo`     | `list[path]`      | `[]`      | Named pipe paths.                                        |
| `bind_ipv6_only`  | enum              | `default` | `default`, `both`, `ipv6-only`.                          |
| `accept`          | `bool`            | `false`   | Whether to spawn a new service instance per connection.   |
| `max_connections`  | `int`             | (none)    | Maximum simultaneous connections. Maps to `MaxConnections=`. |
| `socket_user`     | `string`          | `root`    | Owner of Unix sockets. Validated against user declarations. |
| `socket_group`    | `string`          | `root`    | Group of Unix sockets.                                   |
| `socket_mode`     | `mode`            | `0666`    | Permissions on Unix sockets.                             |
| `service`         | `string`          | (name match) | The service this socket activates. Defaults to same-name service. |

The compiler validates that every port in `listen_stream` and `listen_datagram` is cross-referenced against the firewall declaration (future spec). A socket listening on port 443 with no firewall rule allowing 443 produces a warning.

### s6 Socket Activation

s6 handles socket activation through `s6-ipcserverd` (Unix domain) and `s6-tcpserver` / `s6-tcpserver-access` (TCP). Rather than a separate block type, socket activation in s6 is expressed as a service whose `run` script invokes the appropriate s6 networking tool:

```
init s6 {
    service httpd-socket {
        type = longrun
        
        run = """
            #!/bin/execlineb -P
            s6-tcpserver -v2 -c100 0.0.0.0 80
            s6-tcpserver-access -v2 -l0
            s6-setuidgid apache
            /usr/sbin/httpd-handler
        """

        dependencies = [network]
        
        log { destination = /var/log/s6/httpd }
    }
}
```

This is the s6 way — socket serving is just another process in the supervision tree. Ironclad does not abstract it further. The `run` script is the operator's responsibility. The compiler validates paths and user references but does not parse the execline/shell script content.

---

## Timers and Scheduled Tasks

### systemd Timers

Timers are declared as named blocks inside the init block. A timer activates a corresponding service:

```
init systemd {
    timer certbot-renew {
        description = "Certbot renewal timer"
        on_calendar = "*-*-* 02:00:00"    # systemd calendar syntax
        randomized_delay = 1h
        persistent = true                  # catch up on missed runs after downtime

        install {
            wanted_by = [timers.target]
        }
    }

    service certbot-renew {
        type = oneshot
        exec_start = "/usr/bin/certbot renew --quiet"
        user = root
    }
}
```

#### Timer Properties

| Property           | Type     | Default   | Description                                                    |
|--------------------|----------|-----------|----------------------------------------------------------------|
| `description`      | `string` | (none)    | Human-readable description.                                    |
| `on_calendar`      | `string` | (none)    | Calendar-based schedule (systemd calendar syntax).             |
| `on_boot`          | duration | (none)    | Time after boot. Maps to `OnBootSec=`.                         |
| `on_startup`       | duration | (none)    | Time after systemd start. Maps to `OnStartupSec=`.            |
| `on_unit_active`   | duration | (none)    | Time after the activated unit was last activated. Maps to `OnUnitActiveSec=`. |
| `on_unit_inactive` | duration | (none)    | Time after the activated unit was last deactivated. Maps to `OnUnitInactiveSec=`. |
| `randomized_delay` | duration | `0`       | Random delay added to the trigger time.                        |
| `persistent`       | `bool`   | `false`   | Whether to trigger immediately if the scheduled time was missed. |
| `accuracy`         | duration | `1min`    | Timer accuracy. Maps to `AccuracySec=`.                        |
| `unit`             | `string` | (name match) | Explicit target unit. Defaults to `<name>.service`.         |

### s6 Scheduled Tasks

s6 does not have a built-in timer/scheduler. Scheduled tasks in s6 are either:

1. A `oneshot` service triggered by an external cron-equivalent (declared in the filesystem tree as a crontab or similar)
2. A `longrun` service that sleeps and wakes on schedule using `s6-sleep` or an external scheduler

Ironclad provides a convenience `cron` block for s6 systems that compiles to a crontab entry:

```
init s6 {
    cron certbot-renew {
        schedule = "0 2 * * *"         # standard cron syntax
        command = "/usr/bin/certbot renew --quiet"
        user = root
        random_delay = 3600            # seconds
    }
}
```

The compiler emits a crontab entry in `/etc/cron.d/ironclad-certbot-renew` and validates the path against the filesystem tree. This is not an s6 feature — it is a convenience that the compiler provides because timed execution is a common need regardless of init system.

---

## Targets and Bundles

### systemd Targets

Custom targets can be declared to group services:

```
init systemd {
    target web-stack {
        description = "Web application stack"
        wants = [httpd.service, postgresql.service, redis.service]
        after = [network-online.target]
    }

    service httpd {
        wanted_by = [web-stack.target]
        # ...
    }
}
```

| Property      | Type           | Default | Description                                     |
|---------------|----------------|---------|--------------------------------------------------|
| `description` | `string`       | (none)  | Human-readable description.                      |
| `wants`       | `list[string]` | `[]`    | Services this target pulls in.                   |
| `requires`    | `list[string]` | `[]`    | Hard dependencies.                               |
| `after`       | `list[string]` | `[]`    | Ordering — this target is considered reached after these units. |
| `conflicts`   | `list[string]` | `[]`    | Mutual exclusion.                                |
| `allow_isolate`| `bool`        | `false` | Whether `systemctl isolate` can switch to this target. |

### s6 Bundles

s6-rc bundles group services:

```
init s6 {
    service web-stack {
        type = bundle
        contents = [httpd, postgresql, redis]
    }
}
```

Bundles have `type = bundle` and `contents` listing the member services. The compiler validates that all contents reference declared services.

---

## Hardening Properties (systemd)

The `hardening` block inside a systemd service maps directly to systemd's sandboxing and security directives. These are systemd-specific — s6 does not have equivalent built-in mechanisms (operators achieve similar isolation through the `run` script and external tools like `s6-applyuidgid`, `unshare`, namespace tools, etc.).

```
service myapp {
    hardening {
        # Filesystem isolation
        protect_system = strict        # true | full | strict
        protect_home = true            # true | read-only | tmpfs
        private_tmp = true
        private_devices = true
        read_only_paths = [/etc]
        inaccessible_paths = [/root, /home]
        temporary_filesystem = ["/var:ro"]
        bind_paths = ["/data/myapp:/app/data"]
        bind_read_only_paths = ["/etc/ssl:/app/certs"]

        # Privilege restriction
        no_new_privileges = true
        restrict_suid_sgid = true
        lock_personality = true
        
        # Capability control
        capability_bounding_set = [CAP_NET_BIND_SERVICE]
        ambient_capabilities = [CAP_NET_BIND_SERVICE]

        # Kernel protection
        protect_kernel_tunables = true
        protect_kernel_modules = true
        protect_kernel_logs = true
        protect_control_groups = true
        protect_clock = true
        protect_hostname = true

        # Execution restrictions
        memory_deny_write_execute = true
        restrict_namespaces = true
        restrict_realtime = true
        restrict_address_families = [AF_INET, AF_INET6, AF_UNIX]
        system_call_filter = ["@system-service", "~@privileged"]
        system_call_architectures = [native]
        system_call_error_number = EPERM

        # Network
        private_network = false
        ip_address_allow = ["192.168.1.0/24", "10.0.0.0/8"]
        ip_address_deny = [any]
    }
}
```

All hardening properties are optional. The compiler maps each to its corresponding systemd directive. There is no Ironclad-specific transformation — the property names mirror systemd's directive names, converted from PascalCase to snake_case.

**Security floor enforcement for hardening:**

| Level       | Enforcement                                                                                  |
|-------------|----------------------------------------------------------------------------------------------|
| **Baseline**  | No enforcement. Hardening properties are optional.                                          |
| **Standard**  | Warning if a service runs as `root` without `no_new_privileges = true`.                     |
| **Strict**    | Error if a non-system service lacks `protect_system`, `private_tmp`, `no_new_privileges`. Warning if `system_call_filter` is absent. |
| **Maximum**   | All services must declare a `hardening` block. `capability_bounding_set` is required (no unbounded capabilities). `system_call_filter` is required. `protect_system = strict` is required for all non-PID-1 services. |

---

## Resource Control

### systemd

The `resource_control` block maps to systemd's cgroup resource management:

```
service database {
    resource_control {
        memory_max = 4G
        memory_high = 3G
        memory_low = 1G
        memory_swap_max = 0
        
        cpu_weight = 200
        cpu_quota = 150%              # 150% = 1.5 cores
        allowed_cpus = [0, 1, 2, 3]
        
        tasks_max = 4096
        
        io_weight = 200
        io_read_bandwidth_max = ["/dev/sda 100M"]
        io_write_bandwidth_max = ["/dev/sda 50M"]
        
        ip_accounting = true
        ip_address_allow = ["10.0.0.0/8"]
        ip_address_deny = [any]
        
        oom_policy = stop              # stop | continue | kill
        oom_score_adjust = -500
    }
}
```

### s6

s6 does not have built-in cgroup management. Resource control for s6 services is achieved through the `run` script using `s6-softlimit` or by declaring cgroup configuration via the filesystem:

```
init s6 {
    service database {
        type = longrun
        
        resource_control {
            soft_limit {
                memory = 4G            # RLIMIT_AS
                open_files = 65536     # RLIMIT_NOFILE
                processes = 4096       # RLIMIT_NPROC
                core = 0               # RLIMIT_CORE
                stack = 8M             # RLIMIT_STACK
            }
        }

        run = """
            #!/bin/execlineb -P
            s6-softlimit -m 4294967296 -o 65536 -p 4096
            s6-setuidgid postgres
            /usr/bin/postgres -D /var/lib/pgsql/data
        """
    }
}
```

The `resource_control.soft_limit` block on an s6 service tells the compiler to prepend `s6-softlimit` invocations to the `run` script. If the operator has already included `s6-softlimit` in their `run` script, declaring `soft_limit` is redundant — the compiler does **not** inject `s6-softlimit` if the `run` script already contains the string `s6-softlimit` (simple string match). In that case, the `soft_limit` block serves only as documentation and drift detection metadata.

---

## Drop-in Overrides (systemd)

systemd supports `.d/` drop-in directories for extending unit files without modifying the package-provided originals. Ironclad exposes this via the `dropin` keyword:

```
init systemd {
    dropin httpd "hardening" {
        hardening {
            protect_system = strict
            no_new_privileges = true
            private_tmp = true
        }
    }

    dropin httpd "memory-limit" {
        resource_control {
            memory_max = 2G
        }
    }
}
```

The `dropin` block takes a service name and a dropin name. The compiler emits a file at `/etc/systemd/system/<service>.service.d/<name>.conf` containing only the declared properties, formatted as a systemd unit drop-in.

This is how Ironclad modifies package-provided service units without replacing them. It is functionally equivalent to the `dropin_for` pattern in `FILEOPS_SYNTAX.md` but expressed in the init-native syntax rather than as a file edit.

Drop-ins can coexist with full `service` declarations. When both exist for the same service name, the compiler emits the full unit file and the drop-ins. The drop-ins override the unit file's properties at runtime (standard systemd behavior).

---

## Compiler Output

### systemd

The compiler emits:

- Unit files at `/etc/systemd/system/<name>.service` for each `service` declaration
- Socket files at `/etc/systemd/system/<name>.socket` for each `socket` declaration
- Timer files at `/etc/systemd/system/<name>.timer` for each `timer` declaration
- Target files at `/etc/systemd/system/<name>.target` for each `target` declaration
- Drop-in files at `/etc/systemd/system/<name>.service.d/<dropin>.conf` for each `dropin` declaration
- Journal configuration at `/etc/systemd/journald.conf` from the `journal` block
- Coredump configuration at `/etc/systemd/coredump.conf` from the `coredump` block
- Symlinks for `wanted_by` and `required_by` in the appropriate `.wants/` and `.requires/` directories

All emitted files are marked `content = generated` in the filesystem tree and included in the intermediate manifest for drift detection.

### s6

The compiler emits:

- Service directories under `${rc.source_dir}/<name>/` containing `run`, `finish`, `type`, `notification-fd`, `timeout-up`, `timeout-down`, and `dependencies` as appropriate
- Log service directories under `${rc.source_dir}/<name>-log/` for each service with a `log` block
- Pipeline definitions for service-logger pairs
- Bundle directories under `${rc.source_dir}/<name>/` containing `type` (`bundle`) and `contents`
- Environment directories under `${rc.source_dir}/<name>/env/` for declared environments
- A compiled database at `${rc.compiled_dir}` produced by `s6-rc-compile` invoked during image build
- Crontab files at `/etc/cron.d/ironclad-<name>` for `cron` declarations

---

## Compiler Cross-Validation

| Validation | Description |
|---|---|
| **Init block required** | A system with no `init` block is a compile error. |
| **Single init block** | Only one `init` block per system. Multiple init blocks are a compile error. |
| **User/group exist** | `user` and `group` on services are validated against the system's user and group declarations. |
| **SELinux type exists** | `selinux` on services is validated against the SELinux policy declaration. |
| **Dependency targets valid** | All dependency references resolve to declared services, targets, bundles, or well-known units. |
| **Filesystem paths exist** | Paths in `exec_start`, `environment_file`, `ExecStart`, log destinations, and socket paths are validated against the filesystem tree (warning if not found — may be runtime-created). |
| **Port cross-reference** | Ports in `listen_stream` and `listen_datagram` are cross-referenced against the firewall declaration (warning if no corresponding allow rule). |
| **Package consistency** | If `init = systemd`, the compiler validates that systemd packages are in the system's package list. If `init = s6`, validates s6/s6-rc packages. |
| **Type-specific validation** | `forking` requires `pid_file` or emits a warning. `notify` should have `notify_access`. `dbus` requires `bus_name`. `oneshot` with `remain_after_exit = false` and `wanted_by` emits a warning (service will appear inactive). |
| **Circular dependency** | The compiler builds a dependency graph and errors on cycles. |
| **Socket-service pairing** | Every socket must have a matching service (by name or explicit `service` reference). An orphaned socket is a compile error. |
| **Timer-service pairing** | Every timer must have a matching service. An orphaned timer is a compile error. |
| **Logging required** | At security floor `standard` and above, long-running services without logging declarations produce errors. |

---

## Security Floor Enforcement

| Level        | Enforcement                                                                                                  |
|--------------|--------------------------------------------------------------------------------------------------------------|
| **Baseline** | No enforcement. All properties optional. No warnings for missing hardening or logging.                       |
| **Standard** | Warning for services running as `root` without `no_new_privileges`. Warning for missing logging. Warning for services without `restart` policy. |
| **Strict**   | Standard warnings become errors. All non-root services must have `hardening` block (systemd) or equivalent isolation in `run` script (s6). All longrun services must have logging. `restart` is required. |
| **Maximum**  | Strict plus: `capability_bounding_set` required for all services (systemd). `system_call_filter` required. `protect_system = strict` required for all non-PID-1 services. Every service must declare `selinux`. s6 services must use `s6-setuidgid` or equivalent (compiler scans `run` script for uid switching). |

---

## Reserved Keywords

The following words are reserved in init context:

`init`, `systemd`, `s6`, `raw`, `service`, `socket`, `timer`, `target`, `dropin`, `cron`, `bundle`, `longrun`, `oneshot`, `simple`, `exec`, `forking`, `notify`, `dbus`, `idle`, `run`, `finish`, `up`, `down`, `exec_start`, `exec_stop`, `exec_reload`, `exec_start_pre`, `exec_start_post`, `type`, `description`, `user`, `group`, `selinux`, `after`, `before`, `requires`, `wants`, `conflicts`, `wanted_by`, `required_by`, `restart`, `restart_delay`, `restart_max`, `restart_window`, `timeout_start`, `timeout_stop`, `timeout_up`, `timeout_down`, `environment`, `environment_file`, `environment_dir`, `dependencies`, `contents`, `notification`, `resource_control`, `hardening`, `logging`, `log`, `journal`, `coredump`, `defaults`, `install`, `listen_stream`, `listen_datagram`, `accept`, `on_calendar`, `on_boot`, `persistent`, `default_target`, `scan_dir`, `log_dir`, `pipeline_name`, `max_death_tally`, `min_death_tally_period`, `soft_limit`, `protect_system`, `no_new_privileges`, `private_tmp`, `generated`

These are in addition to language-level reserved keywords defined in `LANGUAGE_SYNTAX.md`.

---

## Grammar Summary (Informative)

```
init_block          = "init" init_backend "{" init_body "}"
init_backend        = "systemd" | "s6" | "raw"
init_body           = init_property* (service_decl | socket_decl | timer_decl 
                    | target_decl | dropin_decl | cron_decl)*

init_property       = property | journal_block | coredump_block | rc_block | defaults_block

service_decl        = "service" identifier "{" service_body "}"
service_body        = property* (resource_control_block | hardening_block 
                    | logging_block | log_block | notification_block 
                    | environment_block | install_block)*

socket_decl         = "socket" identifier "{" socket_body "}"
socket_body         = property* install_block?

timer_decl          = "timer" identifier "{" timer_body "}"
timer_body          = property* install_block?

target_decl         = "target" identifier "{" target_body "}"
target_body         = property*

dropin_decl         = "dropin" identifier string "{" dropin_body "}"
dropin_body         = property* (resource_control_block | hardening_block)*

cron_decl           = "cron" identifier "{" cron_body "}"
cron_body           = property*

journal_block       = "journal" "{" property* "}"
coredump_block      = "coredump" "{" property* "}"
rc_block            = "rc" "{" property* "}"
defaults_block      = "defaults" "{" property* "}"

resource_control_block = "resource_control" "{" property* soft_limit_block? "}"
soft_limit_block    = "soft_limit" "{" property* "}"
hardening_block     = "hardening" "{" property* "}"
logging_block       = "logging" "{" property* "}"
log_block           = "log" "{" property* "}"
notification_block  = "notification" "{" property* "}"
environment_block   = "environment" "=" "{" (identifier "=" expression)* "}"
install_block       = "install" "{" property* "}"

property            = identifier "=" value
value               = string | int | bool | duration | size | mode | list | map | enum_value | reference
duration            = number time_unit
time_unit           = "ms" | "s" | "m" | "h" | "d"
```

---

## Full Example: systemd Web Server

```
system web01 {
    init systemd {
        default_target = "multi-user.target"

        journal {
            storage = persistent
            max_size = 500M
            compress = true
            seal = true
        }

        defaults {
            restart = on-failure
            restart_delay = 5s
        }

        socket httpd {
            listen_stream = [80, 443]
            install { wanted_by = [sockets.target] }
        }

        service httpd {
            description = "Apache HTTP Server"
            type = notify
            exec_start = "/usr/sbin/httpd -DFOREGROUND"
            exec_reload = "/bin/kill -USR1 $MAINPID"

            user = apache
            group = apache
            selinux = httpd_t

            after = [network-online.target]
            wants = [network-online.target]

            environment_file = /etc/sysconfig/httpd

            resource_control {
                memory_max = 2G
                tasks_max = 4096
            }

            hardening {
                protect_system = strict
                protect_home = true
                private_tmp = true
                no_new_privileges = true
                restrict_namespaces = true
                system_call_filter = ["@system-service", "~@privileged"]
                capability_bounding_set = [CAP_NET_BIND_SERVICE]
            }

            install { wanted_by = [multi-user.target] }
        }

        service postgresql {
            description = "PostgreSQL Database Server"
            type = notify
            exec_start = "/usr/bin/postmaster -D /var/lib/pgsql/data"
            
            user = postgres
            group = postgres
            selinux = postgresql_t

            after = [network.target]

            resource_control {
                memory_max = 8G
                memory_high = 6G
            }

            hardening {
                protect_system = full
                private_tmp = true
                no_new_privileges = true
            }

            install { wanted_by = [multi-user.target] }
        }

        timer certbot-renew {
            on_calendar = "*-*-* 02:00:00"
            randomized_delay = 1h
            persistent = true
            install { wanted_by = [timers.target] }
        }

        service certbot-renew {
            type = oneshot
            exec_start = "/usr/bin/certbot renew --quiet"
        }

        target web-stack {
            description = "Full web application stack"
            wants = [httpd.service, postgresql.service]
            after = [network-online.target]
        }
    }
}
```

## Full Example: s6 Container Host

```
system container_host {
    init s6 {
        scan_dir = /run/service
        log_dir = /var/log/s6
        timestamp_format = tai64n

        rc {
            source_dir = /etc/s6-rc/source
            compiled_dir = /etc/s6-rc/compiled
            timeout_up = 30000
            timeout_down = 30000
        }

        defaults {
            restart = on-failure
            restart_delay = 1s
        }

        service network {
            type = oneshot
            up = """
                #!/bin/execlineb -P
                if { ip link set eth0 up }
                if { ip addr add 10.0.0.10/24 dev eth0 }
                ip route add default via 10.0.0.1
            """
            down = """
                #!/bin/execlineb -P
                ip link set eth0 down
            """
        }

        service podman {
            type = longrun
            description = "Podman container runtime socket"
            
            run = """
                #!/bin/execlineb -P
                fdmove -c 2 1
                s6-setuidgid root
                /usr/bin/podman system service --time=0 unix:///run/podman/podman.sock
            """

            dependencies = [network]
            selinux = container_runtime_t

            notification {
                fd = 3
                timeout = 30000
            }

            log {
                destination = /var/log/s6/podman
                max_size = 10M
                max_files = 10
                user = s6log
                group = s6log
            }
        }

        service nextcloud {
            type = longrun
            description = "Nextcloud container"

            run = """
                #!/bin/execlineb -P
                fdmove -c 2 1
                /usr/bin/podman start --attach nextcloud
            """

            finish = """
                #!/bin/execlineb -S0
                /usr/bin/podman stop -t 30 nextcloud
            """

            dependencies = [podman, network]
            user = root
            selinux = container_t

            timeout_up = 60000
            timeout_down = 45000

            log {
                destination = /var/log/s6/nextcloud
                max_size = 50M
                max_files = 20
                user = s6log
                group = s6log
            }
        }

        service matrix {
            type = longrun
            description = "Matrix Conduwuit server"

            run = """
                #!/bin/execlineb -P
                fdmove -c 2 1
                /usr/bin/podman start --attach conduwuit
            """

            finish = """
                #!/bin/execlineb -S0
                /usr/bin/podman stop -t 30 conduwuit
            """

            dependencies = [podman, network]
            selinux = container_t

            log {
                destination = /var/log/s6/matrix
                max_size = 50M
                max_files = 20
                user = s6log
                group = s6log
            }
        }

        service caddy {
            type = longrun
            description = "Caddy reverse proxy"

            run = """
                #!/bin/execlineb -P
                fdmove -c 2 1
                /usr/bin/podman start --attach caddy
            """

            dependencies = [podman, network, nextcloud, matrix]
            selinux = container_t

            log {
                destination = /var/log/s6/caddy
                max_size = 10M
                max_files = 10
                user = s6log
                group = s6log
            }
        }

        service web-stack {
            type = bundle
            contents = [caddy, nextcloud, matrix, podman]
        }
    }
}
```

---

## What This Document Does Not Cover

This specification covers init system and service declaration syntax. The following topics are defined in separate specifications:

- **Storage topology** — Block devices, encryption, volume management, filesystems (`STORAGE_SYNTAX.md`)
- **Filesystem declarations** — Files, directories, merge semantics (`FILESYSTEM_SYNTAX.md`)
- **File operations** — Editing package-provided unit files via structured edits (`FILEOPS_SYNTAX.md`)
- **Language primitives** — Variables, classes, functions, references (`LANGUAGE_SYNTAX.md`)
- **Bootloader configuration** — Boot entries, kernel parameters, UKI (`BOOTLOADER_SYNTAX.md`)
- **SELinux policy** — Policy types, users, MLS parameters (`SELINUX_SYNTAX.md`)
- **Users and groups** — Account declarations that service `user`/`group` properties validate against (future specification)
- **Firewall / network policy** — nftables rules that service port declarations cross-reference against (future specification)
- **Packages** — Package list declarations that init system presence validates against (future specification)
- **Compiler output mapping** — How service declarations map to Containerfile layers and Ansible playbooks (future specification)
- **Runtime agent service monitoring** — How drift detection applies to service state, unit files, and supervision trees (future specification)
