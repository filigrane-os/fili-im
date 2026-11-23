// Keyboard shortcuts of Conversations (F1): everything can be done without the mouse; the kit's card lists how.
import QtQuick
import Fili.Im
import Fili.UI

FiliShortcutsCard {
    groups: [
        { title: Conversations.tr("Moving around"), rows: [
            ["Ctrl+K  Ctrl+N", Conversations.tr("Go to a conversation")],
            ["Alt+←  Alt+→", Conversations.tr("Back / forward through the conversations shown")],
            ["Alt+↓  Alt+↑", Conversations.tr("Next / previous conversation")],
            ["Ctrl+Tab  Ctrl+Shift+Tab", Conversations.tr("Next / previous tab")],
            ["Space", Conversations.tr("Open the message under the pointer in Quick Look (picture, video, video link)")],
            ["Ctrl+F", Conversations.tr("Find in the conversation (all the platforms of a person in the first view)")],
            ["Ctrl+Shift+F", Conversations.tr("Search every message (Esc clears)")],
            ["Ctrl+U", Conversations.tr("Unread conversations only")],
            ["F5  Ctrl+R", Conversations.tr("Refresh")],
            ["Ctrl+Shift+W", Conversations.tr("Window actions")],
            ["Ctrl+Shift+L", Conversations.tr("Debug logs")]
        ] },
        { title: Conversations.tr("Writing"), rows: [
            ["Ctrl+L", Conversations.tr("Write a message")],
            ["Enter  Shift+Enter", Conversations.tr("Send, new line")],
            ["↑", Conversations.tr("Edit your last message (empty field)")],
            ["Ctrl+Shift+U", Conversations.tr("Send files")],
            ["Ctrl+V", Conversations.tr("Paste a picture or files")],
            ["Esc", Conversations.tr("Drop the reply or the edit")]
        ] },
        { title: Conversations.tr("Other"), rows: [
            ["Ctrl+W", Conversations.tr("Close the tab")],
            ["F1  Ctrl+/", Conversations.tr("This list")],
            ["Ctrl+Q", Conversations.tr("Quit")]
        ] }
    ]
}
