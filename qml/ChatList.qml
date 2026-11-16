// The conversation list (or the search results while a query is active): one row per conversation, newest first.
import QtQuick
import Fili.Im

Rectangle {
    id: list
    color: FiliTheme.background
    Rectangle { anchors { top: parent.top; bottom: parent.bottom; right: parent.right } width: FiliTheme.borderWidth; color: FiliTheme.border }

    function dayLabel(ms) {
        const d = new Date(ms), now = new Date();
        if (d.toDateString() === now.toDateString()) return Qt.formatTime(d, "hh:mm");
        const yesterday = new Date(now.getTime() - 86400000);
        if (d.toDateString() === yesterday.toDateString()) return Conversations.tr("Yesterday");
        return Qt.formatDate(d, d.getFullYear() === now.getFullYear() ? "d MMM" : "d MMM yyyy");
    }
    function preview(c) {
        const m = c.last;
        if (!m) return "";
        if (m.deleted) return Conversations.tr("Message deleted");
        const kinds = { image: "Photo", video: "Video", audio: "Voice message", voice: "Voice message", file: "Document", document: "Document", sticker: "Sticker", location: "Location", contact: "Contact" };
        const body = m.text && m.text.plain ? m.text.plain.replace(/\s+/g, " ") : (kinds[m.kind] ? Conversations.tr(kinds[m.kind]) : "");
        return (m.from_me ? Conversations.tr("You") + ": " : (c.kind !== "direct" && m.sender_name ? m.sender_name + ": " : "")) + body;
    }
    readonly property bool searching: win.query !== ""
    readonly property var rows: searching ? win.results : win.rows

    // Find a conversation by name as you type (the header's search looks in the messages)
    Item {
        id: filterBar
        anchors { top: parent.top; left: parent.left; right: parent.right; rightMargin: FiliTheme.borderWidth }
        height: 40
        SearchField {
            id: filterField
            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: FiliTheme.space3; rightMargin: FiliTheme.space3 }
            placeholderText: Conversations.tr("Find a conversation")
            onTextChanged: win.listFilter = text
            Keys.onEscapePressed: { text = ""; focus = false; }
            Keys.onReturnPressed: if (list.rows.length > 0 && list.rows[0].id) win.openChat(list.rows[0].id)
        }
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: FiliTheme.borderWidth; color: FiliTheme.alpha(FiliTheme.border, 0.5) }
    }
    ListView {
        id: view
        anchors { top: filterBar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right; rightMargin: FiliTheme.borderWidth }
        clip: true
        model: list.rows
        boundsBehavior: Flickable.StopAtBounds
        currentIndex: -1
        KineticScroll {}
        delegate: Rectangle {
            id: row
            required property var modelData
            readonly property bool hit: list.searching
            readonly property var chat: hit ? win.chatById(modelData.chat) : modelData
            // The chat the last message belongs to (a search hit: the chat of the hit)
            readonly property bool severalPlatforms: chat ? (hit ? win.groupIds(chat.id).length > 1 : modelData.members !== undefined) : false
            readonly property string platform: {
                const m = hit ? modelData : (modelData.last || null);
                return m ? win.platformOf(win.chatById(hit ? modelData.chat : m.chat)) : (chat ? win.platformOf(chat) : "");
            }
            width: view.width
            height: 56
            color: chat && chat.id === win.current ? FiliTheme.surfaceHover : area.containsMouse ? FiliTheme.alpha(FiliTheme.surfaceHover, 0.5) : "transparent"
            Avatar {
                id: mark
                anchors { left: parent.left; leftMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
                size: 36
                chatId: row.hit ? win.canon(modelData.chat) : modelData.id
                title: row.hit ? (modelData.chat_title || "?") : modelData.title
                alts: row.chat ? win.groupIds(row.chat.id).filter(x => x !== row.chat.id) : []
            }
            Text {
                anchors { left: mark.right; leftMargin: FiliTheme.space3; right: when.left; rightMargin: FiliTheme.space2; top: parent.top; topMargin: 10 }
                text: row.hit ? (modelData.chat_title || "") : modelData.title
                elide: Text.ElideRight
                textFormat: Text.PlainText
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.size
                font.weight: !row.hit && modelData.unread > 0 ? FiliTheme.weightAccent : Font.Normal
                color: FiliTheme.text
            }
            Text {
                id: when
                anchors { right: parent.right; rightMargin: FiliTheme.space3; top: parent.top; topMargin: 11 }
                text: row.hit ? list.dayLabel(modelData.timestamp) : (modelData.last_ts > 0 ? list.dayLabel(modelData.last_ts) : "")
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.muted
            }
            // Which platform the last message went through, just before it
            Rectangle {
                id: via
                visible: win.multi && row.platform !== "" && row.severalPlatforms
                anchors { left: mark.right; leftMargin: FiliTheme.space3; bottom: parent.bottom; bottomMargin: 11 }
                width: visible ? 14 : 0
                height: 14
                radius: 7
                color: win.platformColor(row.platform)
                Text { anchors.centerIn: parent; text: win.platformName(row.platform).charAt(0); font.pixelSize: 9; font.bold: true; color: "white" }
            }
            Text {
                anchors { left: via.visible ? via.right : mark.right; leftMargin: via.visible ? FiliTheme.space2 : FiliTheme.space3; right: badge.visible ? badge.left : parent.right; rightMargin: FiliTheme.space2; bottom: parent.bottom; bottomMargin: 10 }
                text: row.hit ? (modelData.snippet || "").replace(/<[^>]*>/g, "") : list.preview(modelData)
                elide: Text.ElideRight
                textFormat: Text.PlainText
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeList
                color: FiliTheme.textSecondary
            }
            Rectangle {
                id: badge
                visible: !row.hit && modelData.unread > 0
                anchors { right: parent.right; rightMargin: FiliTheme.space3; bottom: parent.bottom; bottomMargin: 9 }
                height: 18
                width: Math.max(18, count.implicitWidth + 10)
                radius: 9
                color: modelData.muted ? FiliTheme.surfaceActive : FiliTheme.accentColor
                Text {
                    id: count
                    anchors.centerIn: parent
                    text: row.hit ? "" : String(modelData.unread)
                    font.family: FiliTheme.font
                    font.pixelSize: FiliTheme.sizeSmall
                    font.weight: FiliTheme.weightAccent
                    color: modelData.muted ? FiliTheme.text : FiliTheme.background
                }
            }
            MouseArea {
                id: area
                anchors.fill: parent
                hoverEnabled: true
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onClicked: mouse => {
                    if (!row.chat) return;
                    if (mouse.button === Qt.RightButton) { const p = area.mapToItem(null, mouse.x, mouse.y); win.openChatMenu(row.chat, p.x, p.y); }
                    else win.openChat(row.chat.id);
                }
            }
            Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: FiliTheme.borderWidth; color: FiliTheme.alpha(FiliTheme.border, 0.5) }
        }
    }
    function focusFilter() { filterField.forceActiveFocus(); filterField.selectAll(); }
    FiliScrollBar { flick: view; anchors { top: view.top; bottom: view.bottom; right: view.right } }
    Text {
        anchors.centerIn: parent
        visible: list.rows.length === 0
        text: list.searching ? Conversations.tr("No message found") : (win.accounts.length === 0 ? Conversations.tr("No account yet") : Conversations.tr("No conversation"))
        font.family: FiliTheme.font
        font.pixelSize: FiliTheme.size
        color: FiliTheme.muted
    }
}
