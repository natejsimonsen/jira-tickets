#!/usr/bin/env bash
# Captures every UI flow as ordered PNGs, headlessly, in light and dark.
#   scripts/capture-flows.sh [out-dir]    (default ./captures/<YYYYMMDD-HHMM>/)
# The Jira token is read with op into the child processes' environment only; it is never written to disk.
# Snapshot mode forces read-only, so no flow can write to Jira.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${1:-$root/captures/$(date +%Y%m%d-%H%M)}"
mkdir -p "$out/logs"
out="$(cd "$out" && pwd)"

echo "building (snapshot, release)..." >&2
(cd "$root" && nix develop --command cargo build --release --features snapshot)
bin="$root/target/release/jira-tickets"

token="$(op read --account "${OP_ACCOUNT:-my.1password.com}" "op://Private/jira-cli JIRA API Credentials/credential")"

manifest="$out/manifest.md"
{
    echo "# Flow captures $(date '+%Y-%m-%d %H:%M')"
    echo
    echo "| PNG | Flow | Theme | Script | Shows |"
    echo "|---|---|---|---|---|"
} >"$manifest"

# Rewrites each `shot:NN-name.png` to `shot:<out>/<flow>-NN-name-<theme>.png`.
rewrite() {
    local flow="$1" theme="$2" script="$3" step result=""
    local IFS=';'
    for step in $script; do
        if [[ "$step" == shot:* ]]; then
            local name="${step#shot:}"
            step="shot:$out/$flow-${name%.png}-$theme.png"
        fi
        result+="${result:+;}$step"
    done
    printf '%s' "$result"
}

# flow <name> <script> <shot.png|what it shows>... ; extra env for the child comes from $flow_env.
# The token is exported only inside the child's subshell.
flow_env=()
flow() {
    local name="$1" script="$2"
    shift 2
    local theme pids=()
    for theme in light dark; do
        local s
        s="$(rewrite "$name" "$theme" "$script")"
        echo "== $name ($theme)" >&2
        (
            cd "$out"
            if [[ -n "$token" ]]; then export JIRA_API_TOKEN="$token"; else unset JIRA_API_TOKEN; fi
            env ${flow_env[@]+"${flow_env[@]}"} JIRA_TICKETS_READONLY=1 JIRA_TICKETS_THEME="$theme" JIRA_TICKETS_SCRIPT="$s" "$bin"
        ) >"$out/logs/$name-$theme.log" 2>&1 &
        pids+=($!)
    done
    for pid in "${pids[@]}"; do
        wait "$pid" || echo "!! $name: process failed (see logs/)" >&2
    done
    local shot
    for shot in "$@"; do
        local png="${shot%%|*}" what="${shot#*|}"
        for theme in light dark; do
            local file="$name-${png%.png}-$theme.png"
            local mark=""
            [[ -f "$out/$file" ]] || mark=" (MISSING)"
            echo "| $file$mark | $name | $theme | \`$script\` | $what |" >>"$manifest"
        done
    done
}

flow issues-default 'wait:500;shot:01-list.png' \
    '01-list.png|Issues list with the default team, sprint, and columns'
flow issues-team-picker 'click:Platform;wait:200;shot:01-open.png' \
    '01-open.png|Team picker ComboBox open'
flow issues-sprint-picker 'click:(active);wait:200;shot:01-open.png' \
    '01-open.png|Sprint picker ComboBox open'
flow issues-epic-picker 'click:All epics;wait:200;shot:01-open.png' \
    '01-open.png|Epic picker ComboBox open'
flow issues-quick-filter 'click:Only Opened;shot:01-updating.png;wait:2500;shot:02-loaded.png' \
    '01-updating.png|Quick filter toggled on, search in flight' \
    '02-loaded.png|Quick filter results loaded'
flow issues-mine-roadmap 'key:m;wait:200;shot:01-roadmap.png;click:Mine;wait:200;shot:02-mine-roadmap.png' \
    '01-roadmap.png|Roadmap toggle (m) on' \
    '02-mine-roadmap.png|Roadmap plus Mine filter'
flow issues-search 'key:cmd+f;type:alpha;wait:200;shot:01-local-filter.png;key:escape;key:cmd+f;key:cmd+a;type:status = Blocked;key:enter;shot:02-updating.png;wait:2500;shot:03-results.png' \
    '01-local-filter.png|Search box focused, "alpha" filtering rows locally' \
    '02-updating.png|JQL search submitted, in flight' \
    '03-results.png|JQL search results'
flow issues-goto 'key:cmd+k;shot:01-box.png;type:CI-1860;shot:02-typed.png;key:escape' \
    '01-box.png|Go-to box (cmd+k) open' \
    '02-typed.png|Go-to box with an issue key typed'
flow issues-detail 'key:j;key:j;wait:1500;shot:01-detail.png;key:j;wait:1500;shot:02-detail-next.png' \
    '01-detail.png|Second row selected, detail pane loaded' \
    '02-detail-next.png|Next row selected, detail pane follows'
flow issues-composer 'key:j;wait:1500;click:Add a comment;type:This is a draft comment that will not be posted;shot:01-draft.png' \
    '01-draft.png|Comment composer with an unposted draft'
flow pickers-transition 'key:j;wait:800;key:t;wait:1500;shot:01-open.png;key:down;shot:02-moved.png;key:enter;wait:800;shot:03-readonly-toast.png' \
    '01-open.png|Transition picker open' \
    '02-moved.png|Transition picker cursor moved down' \
    '03-readonly-toast.png|Confirm blocked by read-only mode, toast shown'
flow pickers-assign 'key:j;wait:800;key:a;wait:1500;shot:01-open.png;type:grace;wait:200;shot:02-filtered.png;key:escape' \
    '01-open.png|Assign picker open' \
    '02-filtered.png|Assign picker filtered by "grace"'
flow pickers-sprint 'key:j;wait:800;key:s;wait:1500;shot:01-open.png;key:escape' \
    '01-open.png|Sprint picker open'
flow pickers-edit 'key:j;wait:800;key:e;wait:300;shot:01-open.png;key:escape' \
    '01-open.png|Edit fields dialog open'
flow issues-rank 'key:j;key:j;wait:800;shot:01-before.png;key:shift+j;wait:300;shot:02-after.png' \
    '01-before.png|Row selected before ranking' \
    '02-after.png|After shift+j rank down (read-only: expect a blocked toast)'
flow help 'key:question;wait:200;shot:01-help.png;key:escape' \
    '01-help.png|Keyboard help overlay'
flow placeholders 'key:cmd+2;shot:01-grooming.png;key:cmd+3;shot:02-team.png;key:cmd+4;shot:03-roadmap.png' \
    '01-grooming.png|Grooming screen (cmd+2)' \
    '02-team.png|Team screen (cmd+3)' \
    '03-roadmap.png|Roadmap screen (cmd+4)'
flow narrow 'width:1000;wait:300;shot:01-narrow-list.png' \
    '01-narrow-list.png|Issues list at 1000 px wide'

# Sign-in: a config with site/email but no token reference, no env token, no saved token file.
signin_cfg="$out/signin-config.toml"
rg -v '^\s*(api_token|op_account)\s*=' "$HOME/.config/jira-tickets/config.toml" >"$signin_cfg"
token=""
flow_env=(JIRA_TICKETS_CONFIG="$signin_cfg" JIRA_TICKETS_TOKEN_FILE="$out/no-such-token")
flow sign-in 'shot:01-signin.png' \
    '01-signin.png|Sign-in screen with no token available'
flow_env=()

count="$(fd -e png . "$out" | wc -l | tr -d ' ')"
echo "done: $count PNGs in $out (manifest: $manifest)" >&2
