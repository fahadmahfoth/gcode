#!/bin/zsh
# gcode history hook for zsh.
#
# Sourced from ~/.zshrc inside the block `gcode --init` writes. This follows the
# same invariants as the bash hook: capture `$?` first, chain onto
# `precmd_functions`, never clobber, and never fail the shell. See
# `shell/gcode.bash` for the full reasoning, which is identical except for how zsh
# exposes precmd.
#
# The hook runs once per prompt. It writes nothing when `GCODE_NO_HISTORY=1` is
# set, skips its own commands, and never captures command output (for tty
# compatibility). One JSONL line per command is recorded, and the Rust store reads
# them verbatim.
#
# The exit status is captured into a variable named `ret`, not `status`. zsh makes
# `status` a read-only special variable aliased to `?`, so `local status=$?` fails
# with "read-only variable: status" on the very first line of the hook — which
# means the hook fails on every prompt and records nothing at all. That was not
# hypothetical; it is what the first version of this file did.

if [[ -n "${_GCODE_HOOK_LOADED:-}" ]]; then
    return 0 2>/dev/null || true
fi
_GCODE_HOOK_LOADED=1

_gcode_disabled() {
    case "${GCODE_NO_HISTORY:-}" in
        1 | true | yes | on) return 0 ;;
    esac
    return 1
}

_gcode_json_escape() {
    local s=$1
    s=${s//\\/\\\\}
    s=${s//\"/\\\"}
    # Every remaining control character in one substitution, matching gcode.bash.
    # RFC 8259 forbids all of U+0000..U+001F, not only the three that were
    # enumerated before.
    s=${s//[[:cntrl:]]/ }
    printf '%s' "$s"
}

_gcode_last_command() {
    # zsh's `history` can show numbers; `fc -ln -1` gives the raw command line
    local h
    h=$(fc -ln -1 2>/dev/null) || return 1
    printf '%s' "$h"
}

_gcode_history_file() {
    printf '%s' "${GCODE_HISTORY_FILE:-$HOME/.gcode/history.jsonl}"
}

_gcode_now() {
    if [[ -z "${_GCODE_TS:-}" ]]; then
        # zsh supports strftime
        _GCODE_TS=$(strftime "%s" 2>/dev/null) || _GCODE_TS=""
        if [[ -z "$_GCODE_TS" ]]; then
            _GCODE_TS_UNSUPPORTED=1
        fi
    fi
    if [[ -n "${_GCODE_TS_UNSUPPORTED:-}" ]]; then
        if [[ "${SECONDS:-0}" != "${_GCODE_TS_SECOND:--1}" ]]; then
            _GCODE_TS=$(date +%s 2>/dev/null) || _GCODE_TS=0
            _GCODE_TS_SECOND=${SECONDS:-0}
        fi
    fi
    printf '%s' "${_GCODE_TS:-0}"
}

_gcode_capture() {
    local ret=$?
    [[ -n "${_GCODE_IN_CAPTURE:-}" ]] && return $ret
    _GCODE_IN_CAPTURE=1

    if ! _gcode_disabled; then
        local cmd cwd file ts esc_cmd esc_cwd dir
        cmd=$(_gcode_last_command)
        case "$cmd" in
            gcode | gcode\ * | _gcode | _gcode\ * | source\ *gcode.bash* | source\ *gcode.zsh* | .\ *gcode.bash* | .\ *gcode.zsh*) ;;
            *)
                if [[ -n "$cmd" ]]; then
                    if [[ "$cmd" == "${_GCODE_LAST_CMD:-}" ]] && [[ "$ret" == "${_GCODE_LAST_STATUS:-}" ]]; then
                        unset _GCODE_IN_CAPTURE
                        return $ret
                    fi
                    cwd=$PWD
                    file=$(_gcode_history_file)
                    dir=${file:h}
                    if [[ "$dir" != "$file" ]] && [[ ! -d "$dir" ]]; then
                        mkdir -p "$dir" 2>/dev/null && chmod 0700 "$dir" 2>/dev/null
                    fi
                    ts=$(_gcode_now)
                    esc_cmd=$(_gcode_json_escape "$cmd")
                    esc_cwd=$(_gcode_json_escape "$cwd")
                    (umask 077 && printf '{"ts":%s,"cmd":"%s","exit":%s,"cwd":"%s","out":""}\n' \
                        "$ts" "$esc_cmd" "$ret" "$esc_cwd" >>"$file") 2>/dev/null
                    if [[ -n "$file" ]] && [[ -f "$file" ]]; then
                        _GCODE_LAST_CMD=$cmd
                        _GCODE_LAST_STATUS=$ret
                    fi
                fi
                ;;
        esac
    fi
    unset _GCODE_IN_CAPTURE
    return $ret
}

# Chain into precmd_functions
if (( ${#precmd_functions} == 0 )); then
    precmd_functions=(_gcode_capture)
else
    precmd_functions+=(_gcode_capture)
fi
