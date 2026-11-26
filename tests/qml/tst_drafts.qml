// The drafts of the composer (qml/Drafts.qml): one per conversation, dropped when sent or emptied. Run by
// scripts/run-qml-tests.sh (make test).
import QtQuick
import QtTest
import "../../qml"

TestCase {
    name: "drafts"
    Drafts { id: d }
    function init() { d.byChat = {}; }

    function test_a_draft_is_found_again_in_its_conversation() {
        d.leave(1, "Hello, see you", [], false);
        compare(d.enter(2).text, "", "another conversation starts empty");
        d.leave(2, "", [], false);
        compare(d.enter(1).text, "Hello, see you");
    }
    function test_each_conversation_keeps_its_own_draft() {
        d.leave(1, "to one", [], false);
        d.leave(2, "to two", [{ path: "/tmp/a.png", name: "a.png" }], false);
        compare(d.enter(1).text, "to one");
        compare(d.enter(2).text, "to two");
        compare(d.enter(2).files.length, 1, "files waiting are kept too");
    }
    function test_files_alone_make_a_draft() {
        d.leave(3, "", [{ path: "/tmp/b.pdf", name: "b.pdf" }], false);
        compare(d.enter(3).files[0].name, "b.pdf");
    }
    function test_sent_or_emptied_drops_the_draft() {
        d.leave(1, "draft", [], false);
        d.leave(1, "", [], false);
        compare(d.enter(1).text, "", "the field was emptied (or the message sent)");
        compare(d.byChat[1], undefined);
    }
    function test_an_edit_in_progress_is_not_a_draft() {
        d.leave(1, "my draft", [], false);
        d.leave(1, "text of the edited message", [], true);
        compare(d.enter(1).text, "", "the edit is dropped, not kept as a draft");
    }
}
