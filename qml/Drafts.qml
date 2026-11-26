// Drafts of the composer, one per conversation (text and files waiting), kept while the window is open: leaving a
// conversation keeps what was written there, coming back finds it again; sending or emptying the field drops it.
// An edit in progress is not a draft (its text is the message being edited). Pure logic, tested in tests/qml/.
import QtQml

QtObject {
    property var byChat: ({})
    // Leaving `chat` with this text and these files in the composer
    function leave(chat, text, files, editing) {
        if (!editing && (text !== "" || files.length > 0))
            byChat[chat] = { text: text, files: files };
        else
            delete byChat[chat];
    }
    // Coming to `chat`: what was left there, or an empty draft
    function enter(chat) {
        return byChat[chat] ?? { text: "", files: [] };
    }
}
