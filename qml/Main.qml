// Window of Conversations: sidebar (inbox filters, accounts), conversation list, open conversations as tabs, footer.
// The window is a client of the daemon (Conversations.call / replied / pushed); it never reads the store.
import QtQuick
import QtQuick.Controls.Basic
import Fili.Im

ApplicationWindow {
    id: win
    property bool windowButtons: Conversations.uiGet("window_buttons") !== "0"
    function toggleWindowButtons() {
        windowButtons = !windowButtons;
        Conversations.uiSet("window_buttons", windowButtons ? "1" : "0");
    }
    // Header and footer follow Settings > Appearance live; search shows the header, a notice shows the footer
    property bool showHeader: Conversations.header()
    property bool showFooter: Conversations.footer()
    property bool searchOpen: false
    property string notice: ""
    property string operation: ""
    readonly property bool headerShown: showHeader || searchOpen || searchInput.text !== ""
    readonly property bool footerShown: showFooter || notice !== "" || operation !== ""
    Connections {
        target: Conversations
        function onThemeChanged() {
            win.showHeader = Conversations.header();
            win.showFooter = Conversations.footer();
        }
    }
    FontLoader { source: "qrc:/qt/qml/Fili/Im/fonts/Jost-Variable.ttf" }
    FontLoader { source: "qrc:/qt/qml/Fili/Im/fonts/JetBrainsMono-Variable.ttf" }
    FontLoader { source: "qrc:/qt/qml/Fili/Im/fonts/FiliIcons.ttf" }
    width: 1100
    height: 680
    minimumWidth: 720
    minimumHeight: 400
    visible: true
    color: FiliTheme.background
    title: "Conversations"

    // ---- State -----------------------------------------------------------------------------------------------
    property var accounts: []
    property var chats: []            // all conversations, as the daemon sorts them (pinned, then recent)
    property string filter: "all"     // all | unread | account:KEY
    property string query: ""
    property var results: []          // search results (messages) while a query is active
    property var tabs: []             // open conversations (chat ids)
    property int current: 0           // chat shown in the conversation view, 0 = none
    property var replyTo: null        // message being answered
    readonly property int unreadTotal: chats.reduce((n, c) => n + (c.unread > 0 ? c.unread : 0), 0)
    readonly property var visibleChats: {
        const f = filter;
        return chats.filter(c => f === "all" || (f === "unread" ? c.unread > 0 : f === "account:" + c.account));
    }
    function chatById(id) { return chats.find(c => c.id === id) || null; }
    function accountByKey(key) { return accounts.find(a => a.key === key) || null; }
    function capsOf(chat) { const a = chat ? accountByKey(chat.account) : null; return a && a.capabilities ? a.capabilities : {}; }
    function filterTitle() {
        if (filter === "all") return Conversations.tr("All Conversations");
        if (filter === "unread") return Conversations.tr("Unread");
        const a = accountByKey(filter.slice(8));
        return a ? (a.self_name || a.name) : "";
    }

    // ---- Requests ---------------------------------------------------------------------------------------------
    property var pending: ({})
    function rpc(method, params, done, failed) {
        const id = Conversations.call(method, JSON.stringify(params || {}));
        pending[id] = { done: done, failed: failed };
    }
    Connections {
        target: Conversations
        function onReplied(id, ok, body) {
            const p = win.pending[id];
            delete win.pending[id];
            if (!p) return;
            let value = null;
            try { value = JSON.parse(body); } catch (e) { }
            if (ok) { if (p.done) p.done(value); }
            else if (p.failed) p.failed(value || {});
            else win.notice = (value && value.message) || Conversations.tr("The request failed");
        }
        function onOnlineChanged() {
            if (Conversations.online) { win.notice = ""; win.reload(); }
            else win.notice = Conversations.tr("The daemon is not running, starting it…");
        }
        function onPushed(text) { win.onEvent(JSON.parse(text)); }
    }
    function reload() {
        rpc("accounts.list", {}, list => { accounts = list; });
        rpc("chats.list", { limit: 500 }, list => { chats = list; });
        if (current) conversation.load();
    }
    function upsert(list, item, key) {
        const i = list.findIndex(x => x[key] === item[key]);
        const copy = list.slice();
        if (i >= 0) copy[i] = item; else copy.push(item);
        return copy;
    }
    function onEvent(e) {
        if (e.event === "chat") {
            const copy = upsert(chats, e.chat, "id");
            copy.sort((a, b) => (b.pinned - a.pinned) || (b.last_ts - a.last_ts) || (b.id - a.id));
            chats = copy;
        } else if (e.event === "message") {
            if (e.message.chat === current) conversation.apply(e.message);
        } else if (e.event === "account") {
            accounts = upsert(accounts, e.account, "key");
        } else if (e.event === "account.removed") {
            accounts = accounts.filter(a => a.key !== e.account);
        } else if (e.event === "pairing") {
            pairDialog.show(e);
        } else if (e.event === "history") {
            operation = e.progress !== undefined && e.progress < 1 ? Conversations.tr("Loading history…") : "";
        } else if (e.event === "account.error") {
            notice = e.message;
        } else if (e.event === "resync") {
            reload();
        }
    }

    // ---- Conversations and tabs ---------------------------------------------------------------------------
    function openChat(id) {
        if (!tabs.includes(id)) tabs = tabs.concat([id]);
        current = id;
        replyTo = null;
        conversation.load();
        const c = chatById(id);
        if (c && c.unread > 0) rpc("mark_read", { chat: id, receipt: false }, null, null);
    }
    function closeTab(id) {
        const i = tabs.indexOf(id);
        if (i < 0) return;
        tabs = tabs.filter(t => t !== id);
        if (current === id) { current = tabs.length ? tabs[Math.min(i, tabs.length - 1)] : 0; if (current) conversation.load(); }
    }
    function cycleTab(step) {
        if (tabs.length < 2) return;
        const i = tabs.indexOf(current);
        openChat(tabs[(i + step + tabs.length) % tabs.length]);
    }
    function cycleChat(step) {
        const list = visibleChats;
        if (!list.length) return;
        const i = list.findIndex(c => c.id === current);
        openChat(list[Math.max(0, Math.min(list.length - 1, i + step))].id);
    }
    function openSearch() { searchOpen = true; searchInput.forceActiveFocus(); }
    function runSearch() {
        query = searchInput.text.trim();
        if (query === "") { results = []; return; }
        rpc("search", { query: query, limit: 50 }, list => { results = list; });
    }

    Component.onCompleted: reload()

    // ---- Menu of the fili-bar (fili-im ctl action NAME) -----------------------------------------------------
    Connections {
        target: Conversations
        function onCommand(name) {
            if (name === "about") aboutCard.show();
            else if (name === "close") Qt.quit();
            else if (name === "window-actions") windowPanel.visible ? windowPanel.close() : windowPanel.open();
            else if (name === "toggle-header") win.showHeader = !win.showHeader;
            else if (name === "toggle-footer") win.showFooter = !win.showFooter;
            else if (name === "toggle-window-buttons") win.toggleWindowButtons();
            else if (name === "search") win.openSearch();
            else if (name === "all") win.filter = "all";
            else if (name === "unread-box") win.filter = "unread";
            else if (name === "next-tab") win.cycleTab(1);
            else if (name === "previous-tab") win.cycleTab(-1);
            else if (name === "close-tab") win.closeTab(win.current);
            else if (name === "next-conversation") win.cycleChat(1);
            else if (name === "previous-conversation") win.cycleChat(-1);
            else if (name === "mark-read") { if (win.current) win.rpc("mark_read", { chat: win.current, receipt: true }); }
            else if (name === "refresh") win.reload();
            else if (name === "reply") conversation.replyToLast();
            else if (name === "focus-composer") conversation.focusComposer();
            else if (name === "add-account") win.rpc("account.add", { platform: "whatsapp" }, a => win.rpc("account.pair", { account: a.key }));
            else if (name === "pair") sidebar.pairFirst();
            else if (name === "new-conversation") win.openSearch();
            else if (name === "copy") conversation.copySelection();
            else if (name === "select-all") conversation.selectAll();
        }
    }
    Shortcut { sequences: ["Ctrl+Q", "Ctrl+W"]; onActivated: win.tabs.length > 0 && win.current ? win.closeTab(win.current) : Qt.quit() }
    Shortcut { sequences: ["Ctrl+Tab"]; onActivated: win.cycleTab(1) }
    Shortcut { sequences: ["Ctrl+Shift+Tab"]; onActivated: win.cycleTab(-1) }
    Shortcut { sequences: ["Alt+Down"]; onActivated: win.cycleChat(1) }
    Shortcut { sequences: ["Alt+Up"]; onActivated: win.cycleChat(-1) }
    Shortcut { sequences: ["Ctrl+F", "Ctrl+N"]; onActivated: win.openSearch() }
    Shortcut { sequences: ["Ctrl+U"]; onActivated: win.filter = win.filter === "unread" ? "all" : "unread" }
    Shortcut { sequences: ["Ctrl+Shift+W"]; onActivated: windowPanel.visible ? windowPanel.close() : windowPanel.open() }
    Shortcut { sequences: ["F5", "Ctrl+R"]; onActivated: win.reload() }
    Shortcut { sequences: ["Ctrl+L"]; onActivated: conversation.focusComposer() }

    // ---- Header ------------------------------------------------------------------------------------------------
    Rectangle {
        id: toolbar
        visible: win.headerShown
        anchors { top: parent.top; left: parent.left; right: parent.right }
        height: visible ? FiliTheme.toolbarHeight : 0
        color: FiliTheme.background
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: FiliTheme.borderWidth; color: FiliTheme.border }
        Column {
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: tools.left; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
            Text {
                width: parent.width
                text: win.query !== "" ? Conversations.tr("Search") : win.filterTitle()
                elide: Text.ElideRight
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.size
                font.weight: FiliTheme.weightAccent
                color: FiliTheme.text
            }
            Text {
                width: parent.width
                text: win.query !== "" ? win.results.length + " " + Conversations.tr("messages")
                    : win.visibleChats.length + " " + Conversations.tr("conversations") + (win.unreadTotal > 0 ? ", " + win.unreadTotal + " " + Conversations.tr("unread") : "")
                elide: Text.ElideRight
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.sizeList
                color: FiliTheme.textSecondary
            }
        }
        Row {
            id: tools
            anchors { right: search.left; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
            spacing: FiliTheme.space1
            IconButton { name: "refresh"; tip: Conversations.tr("Refresh"); onClicked: win.reload() }
            IconButton { name: "mail-open"; tip: Conversations.tr("Mark as read"); enabledState: win.current !== 0; onClicked: win.rpc("mark_read", { chat: win.current, receipt: true }) }
        }
        Rectangle {
            id: closeButton
            visible: win.windowButtons
            anchors { right: parent.right; rightMargin: (toolbar.height - height) / 2; verticalCenter: parent.verticalCenter }
            width: 28
            height: 24
            radius: FiliTheme.radiusControl
            color: closeArea.pressed ? FiliTheme.danger : closeArea.containsMouse ? FiliTheme.alpha(FiliTheme.danger, 0.75) : "transparent"
            WindowGlyph { anchors.centerIn: parent; kind: "close"; width: 16; height: 16; color: closeArea.containsMouse ? "#ffffff" : FiliTheme.text }
            MouseArea { id: closeArea; anchors.fill: parent; hoverEnabled: true; onClicked: Qt.quit() }
            Accessible.role: Accessible.Button
            Accessible.name: Conversations.tr("Close window")
        }
        Rectangle {
            id: windowButton
            visible: win.windowButtons
            anchors { right: closeButton.left; rightMargin: FiliTheme.space1; verticalCenter: parent.verticalCenter }
            width: 28
            height: 24
            radius: FiliTheme.radiusControl
            color: windowArea.pressed ? FiliTheme.surfaceActive : windowArea.containsMouse || windowPanel.visible ? FiliTheme.surfaceHover : "transparent"
            WindowGlyph { anchors.centerIn: parent; kind: "window"; width: 16; height: 16 }
            MouseArea { id: windowArea; anchors.fill: parent; hoverEnabled: true; onClicked: windowPanel.visible ? windowPanel.close() : windowPanel.open() }
            Accessible.role: Accessible.Button
            Accessible.name: Conversations.tr("Window")
        }
        Rectangle {
            id: search
            anchors { right: win.windowButtons ? windowButton.left : parent.right; rightMargin: win.windowButtons ? FiliTheme.space3 : FiliTheme.margin; verticalCenter: parent.verticalCenter }
            width: 180
            height: 26
            radius: FiliTheme.radiusControl
            color: FiliTheme.surface
            border.width: FiliTheme.borderWidth
            border.color: searchInput.activeFocus ? FiliTheme.accentColor : FiliTheme.border
            Text {
                anchors { left: parent.left; leftMargin: FiliTheme.space2; verticalCenter: parent.verticalCenter }
                visible: searchInput.text === ""
                text: Conversations.tr("Search")
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.size
                color: FiliTheme.muted
            }
            TextInput {
                id: searchInput
                anchors { fill: parent; leftMargin: FiliTheme.space2; rightMargin: FiliTheme.space2 }
                verticalAlignment: TextInput.AlignVCenter
                clip: true
                font.family: FiliTheme.font
                font.pixelSize: FiliTheme.size
                color: FiliTheme.text
                selectionColor: FiliTheme.surfaceActive
                onTextChanged: searchTimer.restart()
                Keys.onEscapePressed: { text = ""; win.query = ""; win.results = []; win.searchOpen = false; focus = false }
                Keys.onReturnPressed: win.runSearch()
                Keys.onEnterPressed: win.runSearch()
            }
            Timer { id: searchTimer; interval: 250; onTriggered: win.runSearch() }
        }
    }

    // ---- Body --------------------------------------------------------------------------------------------------
    Sidebar {
        id: sidebar
        visible: win.width >= 760
        anchors { top: toolbar.bottom; bottom: footer.top; left: parent.left }
        width: visible ? FiliTheme.sidebarWidth : 0
    }
    ChatList {
        id: chatList
        anchors { top: toolbar.bottom; bottom: footer.top; left: sidebar.right }
        width: Math.max(240, Math.min(320, win.width * 0.3))
    }
    Item {
        anchors { top: toolbar.bottom; bottom: footer.top; left: chatList.right; right: parent.right }
        TabBar {
            id: tabBar
            anchors { top: parent.top; left: parent.left; right: parent.right }
        }
        Conversation {
            id: conversation
            anchors { top: tabBar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        }
    }

    // ---- Footer ------------------------------------------------------------------------------------------------
    Rectangle {
        id: footer
        visible: win.footerShown
        anchors { bottom: parent.bottom; left: parent.left; right: parent.right }
        height: visible ? FiliTheme.statusHeight : 0
        color: FiliTheme.backgroundDeep
        Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: FiliTheme.borderWidth; color: FiliTheme.border }
        Text {
            anchors { left: parent.left; leftMargin: FiliTheme.margin; right: states.left; rightMargin: FiliTheme.space3; verticalCenter: parent.verticalCenter }
            text: win.notice !== "" ? win.notice : win.operation
            elide: Text.ElideRight
            textFormat: Text.PlainText
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            color: win.notice !== "" ? FiliTheme.danger : FiliTheme.textSecondary
        }
        Text {
            id: states
            anchors { right: parent.right; rightMargin: FiliTheme.margin; verticalCenter: parent.verticalCenter }
            text: win.accounts.map(a => (a.self_name || a.name) + ": " + sidebar.stateWord(a)).join("   ")
            font.family: FiliTheme.font
            font.pixelSize: FiliTheme.sizeSmall
            color: FiliTheme.textSecondary
        }
    }

    PairDialog { id: pairDialog; anchors.fill: parent }
    WindowPanel {
        id: windowPanel
        anchorRight: width - (toolbar.height - closeButton.height) / 2
        anchorTop: toolbar.height + 1
    }
    AboutCard { id: aboutCard; anchors.fill: parent }
}
