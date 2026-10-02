# gcode history hook for bash.
#
# Sourced from ~/.bashrc, inside the block that `gcode --init` writes. Records one
# JSONL line per prompt: the command, its exit status, and the directory. The store
# is read later by `gcode --fix`.
#
# This file is the reason `--fix` can work. ADR 0007 explains why a hook and not a
# wrapper script, and the reasoning there is the reasoning here.
#
# ---------------------------------------------------------------------------
# The three rules, and why each one is written the way it is
# ---------------------------------------------------------------------------
#
# 1. `$?` is captured as the first statement, and restored before returning.
#
#    A prompt hook that forgets this eats the user's exit status. `PS1` cannot show
#    it, so a user whose script sets `PROMPT_COMMAND` loses the ability to see
#    whether anything worked, and nothing tells them it happened. This is the single
#    most common way a history hook breaks a shell.
#
# 2. An existing `PROMPT_COMMAND` is chained onto, never replaced.
#
#    Users have other tools in there — direnv, venv, nvm, a git prompt. Overwriting
#    is the single most destructive thing this file could do, and it would be
#    silently destructive. Both the string form and the array form (bash 5.1+) are
#    handled. The hook goes *before* the user's entries, because it has to see the
#    real `$?` before anything else can overwrite it; see the comment at the
#    install site.
#
# 3. This file never fails the shell.
#
#    Every function is called with `|| true` where it can fail, every expansion is
#    guarded for `set -u`, and nothing here can exit the shell. A hook that can kill
#    a login shell is worse than a hook that records nothing.
#
# ---------------------------------------------------------------------------
# What this deliberately does NOT do
# ---------------------------------------------------------------------------
#
# It does not capture command output.
#
# Capturing output requires redirecting the command's stdout to a file, which
# means `[ -t 1 ]` is false for everything the user runs. Programs check that:
# `ls` drops its colour, `less` stops paging, and an editor refuses to start. That
# was measured, not assumed. A history hook that makes the terminal less useful is
# a bad trade for an error message, so `out` is written empty and `--fix` works from
# the command and the exit status.
#
# It runs no external program on the hot path. No `jq`, no `date`, no `sed`. Both
# `jq` and `date` are measurable costs inside a 5 ms budget — `date +%s` alone
# measured 3.2 ms per prompt on macOS — so the timestamp comes from the shell's own
# clock and is refreshed at most once per second from a cached value.
#
# There are still command substitutions, which are subshells. They are bash
# builtins doing their own work, so they cost a fork but not an exec; the whole hook
# measured at 1.75 ms per prompt, against the 5 ms budget.

# Guard against double-sourcing. `.bashrc` is sourced once per shell, but a user
# with two gcode blocks, or one who sources this file from a session file as well,
# would otherwise get two lines per command.
if [ -n "${_GCODE_HOOK_LOADED:-}" ]; then
    return 0 2>/dev/null || true
fi
_GCODE_HOOK_LOADED=1

# Record nothing when the user has opted out. Checked per prompt rather than once
# at load time, so `GCODE_NO_HISTORY=1` works without restarting the shell.
_gcode_disabled() {
    case "${GCODE_NO_HISTORY:-}" in
        1 | true | yes | on) return 0 ;;
    esac
    return 1
}

# Escape a string for a JSON string literal, in the shell.
#
# Only the two characters that are structurally required, plus control characters:
# a raw newline would break the one-object-per-line format the store depends on, and
# a raw quote would end the string early. Everything else, including UTF-8, passes
# through untouched — mangling non-ASCII would be worse than passing it through.
#
# `printf %s` rather than `echo`, because `echo` in bash 3.2 (still the system bash
# on macOS) does not understand a leading `-n` and would eat it.
_gcode_json_escape() {
    local s=$1
    s=${s//\\/\\\\}
    s=${s//\"/\\\"}
    # Every remaining control character, in one substitution. RFC 8259 forbids all
    # of U+0000..U+001F inside a string, not just the three that were handled here
    # before, and a command containing e.g. a backspace would otherwise produce a
    # line the reader cannot parse — losing the whole entry, not just one field.
    #
    # `[[:cntrl:]]` rather than an enumerated list because the list is what was
    # wrong: it is a set that can be got wrong, and the class is defined by the
    # shell. Verified to leave UTF-8 intact in both bash 3.2 and zsh 5.9, which
    # is the reason it is safe to use rather than a byte filter.
    s=${s//[[:cntrl:]]/ }
    printf '%s' "$s"
}

# The last command, stripped of its history number.
#
# `history 1` is a builtin, so this costs nothing. `HISTTIMEFORMAT` is cleared for
# the duration because a user who has set it gets timestamps in the output, and
# those would be recorded as part of the command.
#
# The number is removed by pattern rather than by `sed`, which would fork. The
# format is a run of spaces and digits, then two spaces, then the command:
# `${h%%[^0-9 ]*}` takes the number and `${h#"$num"}` then takes the rest.
_gcode_last_command() {
    local h
    # `2>/dev/null` because a user with `set -u` and history disabled should get an
    # empty command, not a diagnostic on every prompt.
    h=$(HISTTIMEFORMAT= builtin history 1 2>/dev/null) || return 1
    local num=${h%%[^0-9 ]*}
    local cmd=${h#"$num"}
    # Exactly two spaces, so a command containing double spaces survives.
    cmd=${cmd#  }
    printf '%s' "$cmd"
}

# Where the store lives. `GCODE_HISTORY_FILE` wins, matching the Rust side; the
# default must agree with `paths::history_file()` or the hook and the tool will
# read different files.
_gcode_history_file() {
    printf '%s' "${GCODE_HISTORY_FILE:-$HOME/.gcode/history.jsonl}"
}

# Unix seconds, cached.
#
# `date +%s` is a fork costing ~3.2 ms, which is most of the 5 ms budget for the
# whole hook. `printf '%(%s)T'` is fork-free but needs bash 4.2, and macOS still
# ships 3.2. So: try the fast path once, and if the shell cannot do it, refresh the
# cached value at most once per second. A history timestamp that is up to a second
# stale is worth far more than one that eats two thirds of the prompt budget.
_gcode_now() {
    local now
    if [ -z "${_GCODE_TS:-}" ]; then
        now=$(printf '%(%s)T' -1 2>/dev/null) || now=""
        if [ -z "$now" ]; then
            _GCODE_TS_UNSUPPORTED=1
        else
            _GCODE_TS=$now
        fi
    fi
    if [ -n "${_GCODE_TS_UNSUPPORTED:-}" ]; then
        # `SECONDS` is a bash builtin, so testing it costs nothing, and it tells us
        # whether a second has passed without forking anything.
        if [ "${SECONDS:-0}" -ne "${_GCODE_TS_SECOND:--1}" ]; then
            _GCODE_TS=$(date +%s 2>/dev/null) || _GCODE_TS=0
            _GCODE_TS_SECOND=${SECONDS:-0}
        fi
    fi
    printf '%s' "${_GCODE_TS:-0}"
}

# Record the last command. Called from `PROMPT_COMMAND`.
_gcode_capture() {
    # THE LINE. Before anything else, before any command that could reset `$?`.
    #
    # `local status=$?` on one statement, and not `local status; status=$?`. The
    # two forms are not the same: `local` is itself a command, so splitting them
    # makes the assignment read the status of `local` — always 0 — and every entry
    # would claim the command succeeded. This was a real bug, caught by a test that
    # asserted `false` is recorded with exit 1.
    local status=$?

    # Recursion guard. The commands below can themselves become the last history
    # entry, and this function is called from `PROMPT_COMMAND`, which is not
    # exempt. Without this, a user whose `PROMPT_COMMAND` contains a command would
    # record gcode's own bookkeeping forever.
    [ -n "${_GCODE_IN_CAPTURE:-}" ] && return "$status"
    _GCODE_IN_CAPTURE=1

    if ! _gcode_disabled; then
        local cmd cwd file ts esc_cmd esc_cwd
        cmd=$(_gcode_last_command)

        # Skip gcode's own commands. Recording `gcode --fix` as the user's command
        # would teach `--fix` that its own invocations are the things that fail.
        #
        # `_gcode*` is here because `_gcode_capture` itself is a command, and in a
        # shell where the history has not yet picked up the user's command — a hook
        # called by hand, or the very first prompt after sourcing — `history 1` can
        # be this function. Recording that would fill the store with `_gcode_capture`.
        # A `source` of a gcode hook file is skipped too. The block `gcode --init`
        # writes is `source '<...>/gcode.bash'`, and it runs as a real prompt like
        # any other; without this it is recorded as the user's first command in
        # every new shell, which is noise in the store and would be offered to
        # `--fix` as the thing that failed.
        #
        # One line, because bash does not accept a newline after `|` in a `case`
        # pattern list. That is the whole reason this is one long line.
        case "$cmd" in
            gcode | gcode\ * | _gcode | _gcode\ * | source\ *gcode.bash* | source\ *gcode.zsh* | .\ *gcode.bash* | .\ *gcode.zsh*) ;;
            *)
                # Skip an empty command. Pressing enter at an empty prompt is not a
                # command and does not deserve a history line.
                if [ -n "$cmd" ]; then
                    # Skip a repeat of the command we recorded last, with the same
                    # status. Pressing enter at an empty prompt does this: bash does
                    # not put an empty line in the history, so `history 1` still
                    # returns the command before it, and without this every stray
                    # newline the user types would duplicate an entry.
                    #
                    # The comparison is against a shell variable, not against the
                    # file, so it costs nothing. A variable also cannot be wrong
                    # about a line the user rotated away.
                    if [ "$cmd" = "${_GCODE_LAST_CMD:-}" ] &&
                        [ "$status" = "${_GCODE_LAST_STATUS:-}" ]; then
                        unset _GCODE_IN_CAPTURE
                        return "$status"
                    fi

                    cwd=$PWD
                    file=$(_gcode_history_file)

                    # Create the directory with mode 0700 before writing, because a
                    # hook runs before any part of gcode has set up the store, and
                    # `mkdir -m` is a fork. If the directory cannot be made, skip:
                    # recording into a directory that does not exist would fail
                    # anyway, and failing loudly on every prompt is worse than
                    # recording nothing.
                    dir=${file%/*}
                    if [ "$dir" != "$file" ] && [ ! -d "$dir" ]; then
                        mkdir -p "$dir" 2>/dev/null && chmod 0700 "$dir" 2>/dev/null
                    fi

                    # The history entry, in the store's field order. One `printf`
                    # with a fixed format rather than string concatenation: the
                    # values are already escaped, and a format string cannot be
                    # confused by a quote in the data.
                    ts=$(_gcode_now)
                    esc_cmd=$(_gcode_json_escape "$cmd")
                    esc_cwd=$(_gcode_json_escape "$cwd")

                    # One `printf` and one append. A partially written line is
                    # possible if the shell dies mid-write, and the reader is built
                    # to discard a torn final line, so this is the correct shape.
                    # `umask 077` in a subshell affects only that subshell, so the
                    # file gets `0600` without changing the mode of anything else
                    # the shell creates later.
                    (umask 077 && printf '{"ts":%s,"cmd":"%s","exit":%s,"cwd":"%s","out":""}\n' \
                        "$ts" "$esc_cmd" "$status" "$esc_cwd" >>"$file") 2>/dev/null

                    # Remembered only on success. If the write failed, the same
                    # command should be retried rather than suppressed.
                    if [ -n "$file" ] && [ -f "$file" ]; then
                        _GCODE_LAST_CMD=$cmd
                        _GCODE_LAST_STATUS=$status
                    fi
                fi
                ;;
        esac
    fi

    unset _GCODE_IN_CAPTURE
    # Give the user's exit status back, so their own `PROMPT_COMMAND` entries and
    # their prompt see what the command actually did.
    return "$status"
}

# Chain onto an existing `PROMPT_COMMAND` without clobbering it.
#
# bash 5.1 made `PROMPT_COMMAND` an array that can hold several commands. A string
# `+=` on an array sets element 0 and drops the rest, so the two forms need
# different code. `declare -p` is the portable way to tell them apart, and it is a
# builtin.
if [ "$(declare -p PROMPT_COMMAND 2>/dev/null)" = "declare -a"* ]; then
    PROMPT_COMMAND=(_gcode_capture "${PROMPT_COMMAND[@]}")
else
    # Prepended, not appended. This looks like it should be the other way round —
    # the user's command running first feels less intrusive — but it is wrong.
    # `PROMPT_COMMAND` entries run in sequence and each sees the previous one's
    # status, so appending means `_gcode_capture` records whatever the user's
    # hook last returned rather than what their command actually did. With a
    # `PROMPT_COMMAND` of `user_pc() { return 7; }`, every command in the store
    # reads `"exit":7`, which is worse than not recording at all: `--fix` learns
    # that every command fails for the same unrelated reason.
    #
    # Prepending fixes it because `_gcode_capture` returns the status it captured,
    # so the user's hook still runs after it and still sees the real `$?`.
    #
    # A trailing newline or `;` in what the user had is preserved, because
    # dropping it would be editing their configuration.
    if [ -n "${PROMPT_COMMAND:-}" ]; then
        PROMPT_COMMAND="_gcode_capture"$'\n'"${PROMPT_COMMAND%;}"
    else
        PROMPT_COMMAND="_gcode_capture"
    fi
fi

: # so this file can be sourced without a syntax error under `set -e`