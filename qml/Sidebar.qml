// Sidebar: Inbox (All, Unread) and Accounts (one row per account with its state in words), foldable section headers
// like Facteur's.
import QtQuick
import Fili.Im

Rectangle {
    id: side
    color: FiliTheme.backgroundDeep
    property var folded: ({})
    function fold(key) { const f = Object.assign({}, folded); f[key] = !f[key]; folded = f; }
    function stateWord(a) {
        const words = {
            connected: Conversations.tr("connected"), connecting: Conversations.tr("connecting"),
            needs_pairing: Conversations.tr("needs pairing"), logged_out: Conversations.tr("signed out"),
            banned: Conversations.tr("blocked"), phone_offline: Conversations.tr("phone offline"), error: Conversations.tr("error"), offline: Conversations.tr("offline")
        };
        return words[a.state] || a.state;
    }
    function pairFirst() {
        const a = win.accounts.find(x => x.state === "needs_pairing" || x.state === "logged_out");
        if (a) win.rpc("account.pair", { account: a.key });
    }
    // With several platforms the account is named by its platform (and by its owner when a platform has two accounts)
    function accountLabel(a) {
        if (!win.multi) return a.self_name || side.platformName(a.platform);
        const same = win.accounts.filter(x => x.platform === a.platform).length > 1;
        return side.platformName(a.platform) + (same && a.self_name ? " · " + a.self_name : "");
    }
    function platformName(p) { return win.platformName(p); }

    Rectangle { anchors { top: parent.top; bottom: parent.bottom; right: parent.right } width: FiliTheme.borderWidth; color: FiliTheme.border }

    component SectionHeader: Rectangle {
        property string key
        property string title
        width: side.width - FiliTheme.borderWidth
        height: FiliTheme.sideRowHeight + FiliTheme.space4
        color: "transparent"
        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: FiliTheme.sideRowHeight
            color: headerArea.containsMouse ? FiliTheme.alpha(FiliTheme.surfaceHover, 0.5) : "transparent"
            Text {
                id: arrow
                anchors { left: parent.left; leftMargin: FiliTheme.margin / 2; verticalCenter: parent.verticalCenter }
                text: side.folded[key] ? "▸" : "▾"
                font.pixelSize: FiliTheme.sizeSmall
                color: FiliTheme.muted
            }
            Text {
                anchors { left: arrow.right; leftMargin: FiliTheme.space2; right: parent.right; verticalCenter: parent.verticalCenter }
                text: title
                elide: Text.ElideRight
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeSmall
                font.weight: FiliTheme.weightAccent
                color: FiliTheme.muted
            }
            MouseArea { id: headerArea; anchors.fill: parent; hoverEnabled: true; onClicked: side.fold(key) }
        }
    }
    component Row28: Rectangle {
        id: row
        property string label
        property string detail
        property string count
        property bool selected
        signal clicked()
        signal context(real x, real y)
        width: side.width - FiliTheme.borderWidth
        height: FiliTheme.sideRowHeight
        color: selected ? FiliTheme.surfaceHover : rowArea.containsMouse ? FiliTheme.alpha(FiliTheme.surfaceHover, 0.5) : "transparent"
        Text {
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: tail.left; rightMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
            text: row.label
            elide: Text.ElideRight
            textFormat: Text.PlainText
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.size
            color: FiliTheme.text
        }
        Text {
            id: tail
            anchors { right: parent.right; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
            text: row.count !== "" ? row.count : row.detail
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            color: FiliTheme.muted
        }
        MouseArea {
            id: rowArea
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: mouse => {
                if (mouse.button === Qt.RightButton) { const p = rowArea.mapToItem(null, mouse.x, mouse.y); row.context(p.x, p.y); }
                else row.clicked();
            }
        }
    }

    Flickable {
        id: flick
        anchors { top: parent.top; topMargin: FiliTheme.space3 - FiliTheme.space4; left: parent.left; right: parent.right; bottom: statusRow.top }
        clip: true
        contentHeight: column.height
        boundsBehavior: Flickable.StopAtBounds
        KineticScroll {}
        Column {
            id: column
            width: parent.width
            SectionHeader { key: "inbox"; title: Conversations.tr("Inbox") }
            Row28 {
                visible: !side.folded.inbox
                label: Conversations.tr("All")
                onContext: (x, y) => win.openInboxMenu(x, y)
                selected: win.filter === "all"
                count: win.unreadTotal > 0 ? String(win.unreadTotal) : ""
                onClicked: win.filter = "all"
            }
            Row28 {
                visible: !side.folded.inbox
                label: Conversations.tr("Unread")
                onContext: (x, y) => win.openInboxMenu(x, y)
                selected: win.filter === "unread"
                onClicked: win.filter = "unread"
            }
            SectionHeader { key: "accounts"; title: Conversations.tr("Accounts") }
            Repeater {
                model: side.folded.accounts ? [] : win.accounts
                Row28 {
                    required property var modelData
                    label: side.accountLabel(modelData)
                    detail: modelData.state === "connected" ? win.presenceLabel(win.presenceOf(modelData.key)) : side.stateWord(modelData)
                    onContext: (x, y) => win.openAccountMenu(modelData, x, y)
                    selected: win.filter === "account:" + modelData.key
                    onClicked: win.filter = "account:" + modelData.key
                }
            }
        }
    }
    FiliScrollBar { flick: flick; anchors { top: flick.top; bottom: flick.bottom; right: flick.right } }

    // Presence of the user: a click opens the four choices
    Rectangle {
        id: statusRow
        anchors { left: parent.left; right: parent.right; bottom: addAccount.top }
        width: side.width - FiliTheme.borderWidth
        height: FiliTheme.sideRowHeight
        color: statusArea.containsMouse ? FiliTheme.alpha(FiliTheme.surfaceHover, 0.5) : "transparent"
        Rectangle {
            id: dot
            anchors { left: parent.left; leftMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
            width: 10
            height: 10
            radius: 5
            color: win.presenceColor(win.presence)
        }
        Text {
            anchors { left: dot.right; leftMargin: FiliTheme.space2; right: arrow.left; verticalCenter: parent.verticalCenter }
            text: win.presenceLabel(win.presence)
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.size
            color: FiliTheme.text
        }
        Text { id: arrow; anchors { right: parent.right; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter } text: "▾"; color: FiliTheme.muted }
        MouseArea {
            id: statusArea
            anchors.fill: parent
            hoverEnabled: true
            onClicked: { const p = statusRow.mapToItem(null, FiliTheme.space3, 0); win.openStatusMenu(p.x, p.y - 4 * 26 - 12); }
        }
    }
    Rectangle {
        id: addAccount
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: FiliTheme.sideRowHeight + FiliTheme.space2
        color: "transparent"
        TextButton {
            anchors { left: parent.left; leftMargin: FiliTheme.margin - FiliTheme.space3; verticalCenter: parent.verticalCenter }
            label: Conversations.tr("Add Account…")
            onClicked: win.rpc("account.add", { platform: "whatsapp" }, a => win.rpc("account.pair", { account: a.key }))
        }
    }
}
