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
    property string listFilter: ""    // names typed in the list's filter
    function plain(s) { return (s || "").normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase(); }
    readonly property var visibleChats: {
        const f = filter, q = plain(listFilter.trim());
        return chats.filter(c => (f === "all" || (f === "unread" ? c.unread > 0 : f === "account:" + c.account)) && (q === "" || plain(c.title).includes(q)));
    }
    function chatById(id) { return chats.find(c => c.id === id) || null; }

    // ---- One person, several platforms -----------------------------------------------------------------------
    // Direct conversations with the same name on different accounts are one person (until contacts are linked by hand):
    // one row, one tab, with a view of all of them mixed ("All conversations") or of one platform
    readonly property var groupMap: {
        const byTitle = {};
        chats.forEach(c => { if (c.kind === "direct" && c.title.trim() !== "") { const k = c.title.trim().toLowerCase(); (byTitle[k] = byTitle[k] || []).push(c); } });
        const map = {};
        for (const k in byTitle) {
            const list = byTitle[k];
            if (new Set(list.map(c => c.account)).size < 2) continue;
            const ids = list.map(c => c.id).sort((a, b) => a - b);
            ids.forEach(id => { map[id] = ids; });
        }
        return map;
    }
    function groupIds(id) { return groupMap[id] || [id]; }
    function canon(id) { return groupIds(id)[0]; }
    readonly property var group: groupIds(current).map(chatById).filter(c => c)
    readonly property bool grouped: group.length > 1
    readonly property bool accountFocus: filter.startsWith("account:")   // an account is chosen: its own conversation, no mixed view
    property int scope: 0                 // 0 = all the conversations of the person, else one chat
    property int targetPick: 0            // platform chosen by hand for the next message (All view)
    function scopeIds() { return grouped && scope !== 0 ? [scope] : groupIds(current); }
    readonly property int lastMessageChat: { const m = conversation.messages; return m.length ? m[m.length - 1].chat : current; }
    // Where a message goes: the chat being viewed; in the All view the chosen platform, else where the last message was
    readonly property var targetChat: {
        if (!grouped) return chatById(current);
        if (scope !== 0) return chatById(scope);
        const ids = groupIds(current);
        return chatById(ids.includes(targetPick) ? targetPick : (ids.includes(lastMessageChat) ? lastMessageChat : current));
    }
    function setScope(s) { scope = s; targetPick = 0; replyTo = null; conversation.load(); }
    function groupUnread(id) { return groupIds(id).reduce((n, x) => { const c = chatById(x); return n + (c && c.unread > 0 ? c.unread : 0); }, 0); }
    // The list: a person with several conversations is one row (the latest message, all the unread)
    readonly property var rows: {
        const base = visibleChats;
        if (filter.startsWith("account:")) return base;
        const seen = {};
        const out = [];
        for (const c of base) {
            const ids = groupIds(c.id);
            if (ids.length === 1) { out.push(c); continue; }
            if (seen[ids[0]]) continue;
            seen[ids[0]] = true;
            const members = ids.map(chatById).filter(x => x);
            const latest = members.reduce((a, b) => b.last_ts > a.last_ts ? b : a);
            out.push(Object.assign({}, chatById(ids[0]), {
                unread: members.reduce((n, m) => n + (m.unread > 0 ? m.unread : 0), 0), last: latest.last, last_ts: latest.last_ts,
                muted: members.every(m => m.muted), pinned: members.some(m => m.pinned), members: members }));
        }
        return out;
    }
    function accountByKey(key) { return accounts.find(a => a.key === key) || null; }
    function capsOf(chat) { const a = chat ? accountByKey(chat.account) : null; return a && a.capabilities ? a.capabilities : {}; }
    function filterTitle() {
        if (filter === "all") return Conversations.tr("All Conversations");
        if (filter === "unread") return Conversations.tr("Unread");
        const a = accountByKey(filter.slice(8));
        return a ? (a.self_name || a.name) : "";
    }

    // ---- Message actions (hover bar) ----------------------------------------------------------------------------
    // Forward: pick a destination, then send the text (and the file, downloaded first) there
    function forwardMessage(m) { switchCard.show(m); }
    function sendForward(m, chat) {
        const send = media => rpc("send", { chat: chat, text: m.text.plain, media: media }, () => { notice = Conversations.tr("Message forwarded"); });
        if (m.media) rpc("media.fetch", { message: m.id }, r => send({ path: r.path }));
        else send(undefined);
    }
    // Presence: available (shown online), away (the default: connected, not shown online), busy (no notification
    // pop-ups), offline (the accounts disconnect)
    property string presence: "away"
    readonly property var presences: ["available", "away", "busy", "offline"]
    function presenceLabel(s) {
        return { available: Conversations.tr("Online"), away: Conversations.tr("Away"), busy: Conversations.tr("Busy"), offline: Conversations.tr("Offline") }[s] || s;
    }
    function presenceColor(s) {
        return { available: FiliTheme.accentColor, away: FiliTheme.warning, busy: FiliTheme.danger, offline: FiliTheme.muted }[s] || FiliTheme.muted;
    }
    property var presenceAccounts: ({})      // account key -> its effective state
    function presenceOf(key) { return presenceAccounts[key] || presence; }
    // `account`: the choices for one account (the menu of the sidebar), none: for everyone
    function openStatusMenu(x, y, account) {
        const cur = account ? presenceOf(account) : presence;
        statusMenu.account = account || "";
        statusMenu.openAt(x, y, presences.map(s => ({ id: s, label: (s === cur ? "● " : "○ ") + presenceLabel(s) })));
    }

    // ---- Context menus (right click): a conversation of the list, an account or the inbox of the sidebar ------------
    property string menuKind: ""
    property var menuTarget: null
    property point menuPos: Qt.point(0, 0)
    function openChatMenu(row, x, y) {
        const members = groupIds(row.id).map(chatById).filter(c => c);
        const unread = members.reduce((n, c) => n + (c.unread > 0 ? c.unread : 0), 0);
        const entries = [{ id: "open", label: Conversations.tr("Open") }];
        if (members.length > 1) members.forEach(c => entries.push({ id: "open:" + c.id, label: Conversations.tr("Open") + " " + platformName(platformOf(c)) }));
        if (unread > 0) entries.push({ id: "read", label: Conversations.tr("Mark as Read") });
        if (members.every(c => c.muted)) entries.push({ id: "unmute", label: Conversations.tr("Unmute") });
        else [["3600000", "Mute for 1 hour"], ["28800000", "Mute for 8 hours"], ["604800000", "Mute for 1 week"], ["-1", "Mute Always"]].forEach(m => entries.push({ id: "mute:" + m[0], label: Conversations.tr(m[1]) }));
        entries.push({ id: members.some(c => c.pinned) ? "unpin" : "pin", label: members.some(c => c.pinned) ? Conversations.tr("Unpin") : Conversations.tr("Pin to the Top") });
        entries.push({ id: "info", label: Conversations.tr("Contact Info…") });
        entries.push({ id: "find", label: Conversations.tr("Find in Conversation…") });
        menuKind = "chat"; menuTarget = { id: row.id, members: members };
        ctxMenu.openAt(x, y, entries);
    }
    function openAccountMenu(a, x, y) {
        const cur = presenceOf(a.key);
        const entries = presences.map(s => ({ id: "st:" + s, label: (s === cur ? "● " : "○ ") + presenceLabel(s) }));
        entries.push({ id: "only", label: Conversations.tr("Show Only This Account") });
        if (chats.some(c => c.account === a.key && c.unread > 0)) entries.push({ id: "read", label: Conversations.tr("Mark All as Read") });
        if (a.state === "needs_pairing" || a.state === "logged_out") entries.push({ id: "pair", label: Conversations.tr("Link Account…") });
        else if (cur !== "offline") entries.push({ id: "reconnect", label: Conversations.tr("Reconnect") });
        entries.push({ id: "remove", label: Conversations.tr("Remove Account…") });
        menuKind = "account"; menuTarget = a; menuPos = Qt.point(x, y);
        ctxMenu.openAt(x, y, entries);
    }
    // Add Account: the installed connectors (a single one is added at once)
    function addAccount(x, y) {
        rpc("platforms.list", {}, list => {
            const ps = list.map(p => p.platform).filter(p => p !== "fake");
            if (ps.length === 1) return linkAccount(ps[0]);
            menuKind = "add"; menuTarget = null;
            ctxMenu.openAt(x, y, ps.map(p => ({ id: p, label: platformName(p) })));
        });
    }
    // The pairing dialog opens at once (waiting for the code), then shows what the daemon sends
    function startPairing(key, method) {
        pairDialog.show({ account: key, kind: "wait", data: "" });
        rpc("account.pair", method ? { account: key, method: method } : { account: key }, null, e => { pairDialog.hide(); notice = (e && e.message) || Conversations.tr("The request failed"); });
    }
    function linkAccount(platform) { rpc("account.add", { platform: platform }, a => startPairing(a.key)); }
    function openInboxMenu(x, y) {
        menuKind = "inbox"; menuTarget = null;
        ctxMenu.openAt(x, y, [{ id: "read", label: Conversations.tr("Mark All as Read") }]);
    }
    function markAllRead(list) { list.filter(c => c.unread > 0).forEach(c => rpc("mark_read", { chat: c.id, receipt: true })); }
    function runContext(id) {
        const t = menuTarget;
        if (menuKind === "chat") {
            if (id === "open") openChat(t.id);
            else if (id.startsWith("open:")) { openChat(t.id); setScope(parseInt(id.slice(5))); }
            else if (id === "read") markAllRead(t.members);
            else if (id === "unmute") t.members.forEach(c => rpc("chat.mute", { chat: c.id, until: 0 }));
            else if (id.startsWith("mute:")) { const ms = parseInt(id.slice(5)); t.members.forEach(c => rpc("chat.mute", { chat: c.id, until: ms < 0 ? -1 : Date.now() + ms })); }
            else if (id === "pin" || id === "unpin") t.members.forEach(c => rpc("chat.pin", { chat: c.id, pinned: id === "pin" }));
            else if (id === "info") { openChat(t.id); showContact(chatById(t.id)); }
            else if (id === "find") { openChat(t.id); Qt.callLater(() => conversation.openFind()); }
        } else if (menuKind === "account") {
            if (id.startsWith("st:")) rpc("presence.set", { account: t.key, state: id.slice(3) });
            else if (id === "only") filter = "account:" + t.key;
            else if (id === "read") markAllRead(chats.filter(c => c.account === t.key));
            else if (id === "pair") startPairing(t.key);
            else if (id === "reconnect") rpc("account.reconnect", { account: t.key });
            else if (id === "remove") {
                // Second step: removing unlinks the device and deletes the local history, so ask once
                const pos = menuPos;
                Qt.callLater(() => { menuKind = "remove"; menuTarget = t; ctxMenu.openAt(pos.x, pos.y, [
                    { id: "confirm", label: Conversations.tr("Remove and Delete Its Messages"), danger: true },
                    { id: "cancel", label: Conversations.tr("Cancel") }]); });
            }
        } else if (menuKind === "add") {
            linkAccount(id);
        } else if (menuKind === "remove") {
            if (id === "confirm") rpc("account.remove", { account: t.key }, () => { if (filter === "account:" + t.key) filter = "all"; });
        } else if (menuKind === "inbox") {
            if (id === "read") markAllRead(chats);
        }
    }
    // Profile pictures: asked once per conversation, a few at a time by the daemon
    property var avatars: ({})
    property var avatarAsked: ({})
    function wantAvatar(id) {
        if (avatarAsked[id]) return;
        avatarAsked[id] = true;
        rpc("avatar.fetch", { chat: id }, r => { const a = Object.assign({}, avatars); a[id] = r.path; avatars = a; }, e => { if (e && e.code === "not_connected") delete avatarAsked[id]; });
    }
    // Several platforms: a conversation says which one it is on, and the same person on another one is offered
    readonly property bool multi: { const p = {}; accounts.forEach(a => { p[a.platform] = true; }); return Object.keys(p).length > 1; }
    function platformOf(c) { const a = c ? accountByKey(c.account) : null; return a ? a.platform : ""; }
    function platformName(p) { return ({ whatsapp: "WhatsApp", gmessages: "Google Messages", telegram: "Telegram" })[p] || p; }
    function platformColor(p) { return ({ whatsapp: "#2fa84f", gmessages: "#1a73e8", telegram: "#2a8fd6" })[p] || FiliTheme.muted; }
    function siblings(c) {
        if (!c || c.kind !== "direct") return [];
        const n = c.title.trim().toLowerCase();
        return chats.filter(x => x.id !== c.id && x.kind === "direct" && x.account !== c.account && x.title.trim().toLowerCase() === n);
    }
    function showContact(chat) { contactCard.show(); }
    // Quick Look on the media of the open conversation
    readonly property bool overlayOpen: quickLookCard.visible || contactCard.visible || switchCard.visible || emojiPicker.visible || aboutCard.visible || shortcutsCard.visible || pairDialog.visible || ctxMenu.visible || statusMenu.visible
    // Where each video page was left in a bubble (ms): Quick Look continues from there; opening it stops the bubbles
    property var videoPositions: ({})
    signal stopInline()
    function quickLook(id, at) { quickLookCard.open(conversation.messages, id, at !== undefined ? at : (videoPositions[id] || 0)); }
    // React with an emoji of the full list
    property var emojiTarget: null
    function pickEmoji(m) { emojiTarget = m; emojiPicker.show(); }

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
        rpc("presence.get", {}, r => { presence = r.state; presenceAccounts = r.accounts || {}; });
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
            if (scopeIds().includes(e.message.chat)) conversation.apply(e.message);
        } else if (e.event === "presence") {
            presence = e.state;
            presenceAccounts = e.accounts || {};
        } else if (e.event === "account") {
            accounts = upsert(accounts, e.account, "key");
        } else if (e.event === "account.removed") {
            accounts = accounts.filter(a => a.key !== e.account);
            chats = chats.filter(c => c.account !== e.account);
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
        id = canon(id);
        if (!tabs.includes(id)) tabs = tabs.concat([id]);
        current = id;
        scope = 0;
        if (accountFocus && groupIds(id).length > 1) {
            const mine = groupIds(id).map(chatById).find(c => c && c.account === filter.slice(8));
            if (mine) scope = mine.id;
        }
        targetPick = 0;
        replyTo = null;
        conversation.load();
        groupIds(id).forEach(x => { const c = chatById(x); if (c && c.unread > 0) rpc("mark_read", { chat: x, receipt: "auto" }, null, null); });
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
        const list = rows;
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
            if (name.startsWith("open-chat:")) {
                // A conversation of a person who has several: its own platform view (a notification of that platform)
                const id = parseInt(name.slice(10));
                win.openChat(id);
                if (win.grouped && id !== win.current) win.setScope(id);
                win.raise(); win.requestActivate();
            }
            else if (name === "about") aboutCard.show();
            else if (name === "close") Qt.quit();
            else if (name === "window-actions") windowPanel.visible ? windowPanel.close() : windowPanel.open();
            else if (name === "toggle-header") win.showHeader = !win.showHeader;
            else if (name === "toggle-footer") win.showFooter = !win.showFooter;
            else if (name === "toggle-window-buttons") win.toggleWindowButtons();
            else if (name === "search") { if (win.current) conversation.openFind(); else win.openSearch(); }
            else if (name === "search-all") win.openSearch();
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
            else if (name.startsWith("status-")) rpc("presence.set", { state: name.slice(7) });
            else if (name === "quick-look") conversation.quickLookLast();
            else if (name === "record-voice") conversation.toggleRecording();
            else if (name === "contact-info") { const c = chatById(current); if (c) showContact(c); }
            else if (name === "add-account") addAccount(win.width / 2 - 80, win.height / 3);
            else if (name === "pair") sidebar.pairFirst();
            else if (name === "new-conversation") switchCard.show();
            else if (name === "shortcuts") shortcutsCard.show();
            else if (name === "copy") conversation.copySelection();
            else if (name === "select-all") conversation.selectAll();
        }
    }
    Shortcut { sequences: ["Ctrl+Q", "Ctrl+W"]; onActivated: win.tabs.length > 0 && win.current ? win.closeTab(win.current) : Qt.quit() }
    Shortcut { sequences: ["Ctrl+Tab"]; onActivated: win.cycleTab(1) }
    Shortcut { sequences: ["Ctrl+Shift+Tab"]; onActivated: win.cycleTab(-1) }
    Shortcut { sequences: ["Alt+Down"]; onActivated: win.cycleChat(1) }
    Shortcut { sequences: ["Alt+Up"]; onActivated: win.cycleChat(-1) }
    // Ctrl+F looks in the open conversation (or in all the conversations of the person), Ctrl+Shift+F in every message
    Shortcut { sequences: ["Ctrl+F"]; onActivated: win.current ? conversation.openFind() : win.openSearch() }
    Shortcut { sequences: ["Ctrl+Shift+F"]; onActivated: win.openSearch() }
    Shortcut { sequences: ["Ctrl+K", "Ctrl+N"]; onActivated: switchCard.show() }
    Shortcut { sequences: ["F1", "Ctrl+/"]; onActivated: shortcutsCard.show() }
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
            // Where it matters: write to someone (Ctrl+K), and the user's status (a click opens the choices)
            IconButton { name: "compose"; tip: Conversations.tr("New conversation") + "  Ctrl+K"; onClicked: switchCard.show() }
            IconButton {
                id: statusButton
                tip: Conversations.tr("Status") + ": " + win.presenceLabel(win.presence)
                onClicked: { const p = statusButton.mapToItem(null, 0, statusButton.height + 2); win.openStatusMenu(p.x - 120, p.y); }
                Rectangle { anchors.centerIn: parent; width: 12; height: 12; radius: 6; color: win.presenceColor(win.presence) }
            }
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
            Rectangle {
                id: searchClear
                visible: searchInput.text !== ""
                anchors { right: parent.right; rightMargin: 3; verticalCenter: parent.verticalCenter }
                width: 18
                height: 18
                radius: 9
                color: searchClearArea.containsMouse ? FiliTheme.surfaceHover : "transparent"
                Icon { anchors.centerIn: parent; name: "x"; size: 12; color: FiliTheme.textSecondary }
                MouseArea { id: searchClearArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { searchInput.text = ""; win.query = ""; win.results = []; searchInput.forceActiveFocus(); } }
            }
            TextInput {
                id: searchInput
                anchors { fill: parent; leftMargin: FiliTheme.space2; rightMargin: searchClear.visible ? 24 : FiliTheme.space2 }
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
            text: win.accounts.map(a => (win.multi ? platformName(a.platform) + " " : "") + (a.self_name || a.name) + ": " + sidebar.stateWord(a)).join("   ")
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
    ShortcutsCard { id: shortcutsCard; anchors.fill: parent }
    SwitchCard { id: switchCard; anchors.fill: parent; onForwardTo: (m, chat) => win.sendForward(m, chat) }
    ContactCard { id: contactCard; anchors.fill: parent }
    QuickLook { id: quickLookCard; anchors.fill: parent }
    PopupMenu {
        id: statusMenu
        property string account: ""
        anchors.fill: parent
        onActivated: id => win.rpc("presence.set", account !== "" ? { account: account, state: id } : { state: id })
    }
    PopupMenu { id: ctxMenu; anchors.fill: parent; onActivated: id => win.runContext(id) }
    EmojiPicker { id: emojiPicker; anchors.fill: parent; onPicked: e => { if (win.emojiTarget) win.rpc("react", { message: win.emojiTarget.id, emoji: e }); } }
}
