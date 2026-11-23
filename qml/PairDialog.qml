// Pairing: the QR code the phone scans (Linked devices), or a pairing code; follows the daemon's `pairing` events.
import QtQuick
import Fili.Im
import Fili.UI

FiliDialog {
    id: dialog
    property string account: ""
    property string kind: ""
    property string code: ""
    property var rows: []
    z: 220
    showCancel: false
    confirmLabel: Conversations.tr("Close")
    title: Conversations.tr("Link %1").replace("%1", dialog.platformLabel)

    readonly property string platform: { const a = win.accountByKey(account); return a ? a.platform : "whatsapp"; }
    readonly property string platformLabel: win.platformName(platform)
    function show(e) {
        account = e.account;
        kind = e.kind;
        code = e.data;
        rows = kind === "qr" ? JSON.parse(Conversations.qrRows(code)) : [];
        open();
    }
    function hide() { close(); }
    Connections {
        target: win
        // Paired: the account turns connected, the dialog has nothing more to say
        function onAccountsChanged() {
            const a = win.accountByKey(dialog.account);
            if (dialog.visible && a && a.state === "connected") dialog.hide();
        }
    }
    Text {
        width: parent.width
        wrapMode: Text.Wrap
        text: dialog.kind === "wait" ? Conversations.tr("Contacting %1…").replace("%1", dialog.platformLabel)
            : dialog.kind === "browser" ? Conversations.tr("Sign in to your Google account in the window that just opened. It closes by itself once you are signed in.")
            : dialog.kind === "emoji" ? Conversations.tr("On your phone, Google Messages asks to confirm this emoji. Tap it there.")
            : dialog.platform === "gmessages" ? Conversations.tr("On your phone, open Messages, tap your profile picture, then Device pairing > QR code scanner, and scan this code.")
            : dialog.kind === "qr" ? Conversations.tr("On your phone, open WhatsApp, then Settings > Linked devices > Link a device, and scan this code.")
                                   : Conversations.tr("On your phone, open WhatsApp, then Settings > Linked devices > Link with phone number, and type this code.")
        font.family: FiliTheme.font
        font.pixelSize: FiliTheme.size
        color: FiliTheme.textSecondary
    }
    Rectangle {
        visible: dialog.kind === "qr"
        anchors.horizontalCenter: parent.horizontalCenter
        width: 260
        height: 260
        color: "#ffffff"
        Canvas {
            anchors.centerIn: parent
            width: 240
            height: 240
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                const n = dialog.rows.length;
                if (n === 0) return;
                const quiet = 2, cell = width / (n + 2 * quiet);
                ctx.fillStyle = "#000000";
                for (let y = 0; y < n; y++)
                    for (let x = 0; x < n; x++)
                        if (dialog.rows[y][x] === "1") ctx.fillRect(Math.floor((x + quiet) * cell), Math.floor((y + quiet) * cell), Math.ceil(cell), Math.ceil(cell));
            }
            // Painted again when it appears (the dialog first opens without a code) and when the code changes
            onVisibleChanged: if (visible) requestPaint()
            onAvailableChanged: if (available) requestPaint()
            Connections { target: dialog; function onRowsChanged() { requestPaint(); } }
        }
    }
    Text {
        visible: dialog.kind === "emoji"
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: dialog.code
        font.pixelSize: 96
    }
    Text {
        visible: dialog.kind === "code"
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: dialog.code
        font.family: FiliTheme.mono
        font.pixelSize: FiliTheme.sizeTitle
        color: FiliTheme.text
    }
    FiliButton {
        x: parent.width - width
        visible: dialog.platform === "gmessages" && dialog.kind !== "qr" && dialog.kind !== "emoji"
        label: Conversations.tr("Use a QR Code Instead")
        onClicked: win.startPairing(dialog.account, "qr")
    }
}
