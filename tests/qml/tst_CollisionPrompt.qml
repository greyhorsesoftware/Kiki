import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// The prompt a job raises when the destination already exists (plan 04): it appears on the
// daemon's event, each button answers with its choice, and "apply to all" rides along.
TestCase {
    id: tc
    name: "CollisionPrompt"
    when: windowShown
    visible: true
    width: 600; height: 400

    UI.CollisionPrompt { id: dlg; anchors.fill: parent }

    function prompt(name) {
        return {
            event: "Prompt", job: 4, kind: "collision",
            uri: "file:///home/t/" + name,
            existing: { size: 10, mtime: 1700000000000 },
            incoming: { size: 20, mtime: 1700000900000 }
        }
    }

    function init() { Wire.reset(); Kiki.Jobs.prompt = null }
    function cleanup() { Kiki.Jobs.prompt = null }

    function test_it_stays_out_of_the_way_until_a_job_hits_a_collision() {
        compare(dlg.visible, false)
        Wire.emitEvent(prompt("a.txt"))
        compare(dlg.visible, true)
        compare(dlg.prompt.job, 4)
    }

    function test_replace_answers_with_that_choice_and_closes() {
        Wire.emitEvent(prompt("a.txt"))
        mouseClick(findChild(dlg, "collision-replace"))
        const req = Wire.last("PromptReply")
        verify(req !== null)
        compare(req.choice, "replace")
        compare(req.job, 4)
        compare(req.applyToAll, false)
        compare(dlg.visible, false)
    }

    function test_skip_answers_with_skip() {
        Wire.emitEvent(prompt("a.txt"))
        mouseClick(findChild(dlg, "collision-skip"))
        compare(Wire.last("PromptReply").choice, "skip")
    }

    function test_keep_both_answers_with_keep_both() {
        Wire.emitEvent(prompt("a.txt"))
        mouseClick(findChild(dlg, "collision-keepBoth"))
        compare(Wire.last("PromptReply").choice, "keepBoth")
    }

    // The tick box is the difference between answering once and answering for the whole job.
    function test_apply_to_all_rides_along_with_the_choice() {
        Wire.emitEvent(prompt("a.txt"))
        mouseClick(findChild(dlg, "collision-all"))
        mouseClick(findChild(dlg, "collision-replace"))
        compare(Wire.last("PromptReply").applyToAll, true)
    }

    // A tick meant for one job must not silently replace files in the next.
    function test_apply_to_all_does_not_carry_into_the_next_prompt() {
        Wire.emitEvent(prompt("a.txt"))
        mouseClick(findChild(dlg, "collision-all"))
        mouseClick(findChild(dlg, "collision-replace"))
        Wire.emitEvent(prompt("b.txt"))
        mouseClick(findChild(dlg, "collision-replace"))
        compare(Wire.last("PromptReply").applyToAll, false)
    }

    function test_the_second_collision_opens_it_again() {
        Wire.emitEvent(prompt("a.txt"))
        mouseClick(findChild(dlg, "collision-skip"))
        compare(dlg.visible, false)
        Wire.emitEvent(prompt("b.txt"))
        compare(dlg.visible, true)
        compare(Wire.count("PromptReply"), 1)
    }
}
