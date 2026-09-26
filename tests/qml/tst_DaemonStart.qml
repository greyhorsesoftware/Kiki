import QtQuick
import QtTest
import KikiTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/version.js" as Version

// The daemon is the window's engine, and the window starts it (docs/0.3.0/01-daemon-on-demand.md):
// nothing answering the socket means no daemon is running, so run one — once per silence, not
// once per retry — and go on trying the socket until it answers.
TestCase {
    name: "DaemonStart"

    function init() {
        Wire.reset()
        Kiki.Daemon.starting = false
        Kiki.Daemon.startError = ""
        Kiki.Daemon.ready = false
        Kiki.Daemon.retry.stop()
        Kiki.Daemon.attempts = 0
    }
    function cleanup() { Kiki.Daemon.starting = false; Kiki.Daemon.startError = ""; Kiki.Daemon.ready = false; Kiki.Daemon.retry.stop() }

    function test_the_binary_is_the_one_beside_the_shell_unless_named() {
        // `KIKI_DAEMON` names another — what `make run` sets to the checkout's; the stub
        // environment names nothing, so it is the `kikid` on PATH.
        compare(Kiki.Daemon.daemonBinary, "kikid")
    }

    function test_nothing_answering_starts_one_and_only_one() {
        Kiki.Daemon.startDaemon()
        compare(Wire.startCount(), 1, "a daemon is started")
        compare(Wire.lastStart()[0], Kiki.Daemon.daemonBinary)
        verify(Kiki.Daemon.starting, "and is known to be starting")
        // The retry ticks on while the daemon binds; it must not start a second one.
        Kiki.Daemon.startDaemon()
        Kiki.Daemon.startDaemon()
        compare(Wire.startCount(), 1, "once per silence, not once per tick")
    }

    // The bug this pair exists for (2026-09-26): the window started a daemon and then sat there
    // beside it, connected to nothing, because `retry` was only ever started when a connection
    // was LOST — and a socket that has never connected has none to lose.
    function test_starting_one_also_starts_looking_for_it() {
        compare(Kiki.Daemon.retry.running, false, "nothing to look for yet")
        Kiki.Daemon.startDaemon()
        compare(Kiki.Daemon.retry.running, true, "the window goes looking for what it started")
    }
    function test_the_first_second_is_tried_hard_then_patiently() {
        // A daemon binds in about 10 ms; a cold start that waited out the slow cadence spent
        // most of a second doing nothing.
        Kiki.Daemon.tryAgain()
        compare(Kiki.Daemon.attempts, 0)
        verify(Kiki.Daemon.retry.interval <= 50, "fast at first: " + Kiki.Daemon.retry.interval)
        Kiki.Daemon.attempts = Kiki.Daemon.fastTries
        verify(Kiki.Daemon.retry.interval >= 500, "patient after that: " + Kiki.Daemon.retry.interval)
        // Whatever it was doing, an answer stops it.
        Kiki.Daemon.ready = true
        Kiki.Daemon.tryAgain()
        compare(Kiki.Daemon.retry.running, false, "a daemon that answered is not hunted")
    }
    function test_a_daemon_that_answered_is_never_started_over() {
        Kiki.Daemon.ready = true
        Kiki.Daemon.startDaemon()
        compare(Wire.startCount(), 0, "the socket answers: nothing to start")
    }

    function test_standing_down_is_not_a_failure_but_a_crash_is_said() {
        // Exit 0 without this window connecting is the other window's daemon winning the race:
        // ordinary, and nothing to tell anyone about.
        Kiki.Daemon.startDaemon()
        Kiki.Daemon.starter.finish(0)
        compare(Kiki.Daemon.startError, "", "a stand-down is quiet")
        verify(!Kiki.Daemon.starting, "and the window may start one again")

        // Anything else is an engine that would not run, which a window must not sit quietly on.
        Kiki.Daemon.startDaemon()
        Kiki.Daemon.starter.finish(127)
        verify(Kiki.Daemon.startError.indexOf("127") >= 0, Kiki.Daemon.startError)
        verify(Kiki.Daemon.startError.indexOf(Kiki.Daemon.daemonBinary) >= 0, Kiki.Daemon.startError)
    }

    function test_the_failure_is_said_in_the_windows_language() {
        Kiki.T.language = "es"
        Kiki.Daemon.startDaemon()
        Kiki.Daemon.starter.finish(1)
        verify(Kiki.Daemon.startError.indexOf("no se pudo iniciar") >= 0, Kiki.Daemon.startError)
        Kiki.T.language = "en"
    }
}
