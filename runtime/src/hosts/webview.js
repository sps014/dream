/**
 * `system.webview` is native-only (wry). Browser/Node stubs report unsupported so the Dream
 * module contract stays complete.
 */

function unsupportedCreate() {
  return -1;
}

function unsupportedCode() {
  return 1;
}

function unsupportedPoll() {
  // count = 0
  return new TextEncoder().encode("0\n");
}

function unsupportedEval() {
  return new TextEncoder().encode("1\nunsupported on this host\n");
}

export function makeWebviewHost() {
  return {
    webviewCreate: (_title, _width, _height) => unsupportedCreate(),
    webviewLoadUrl: (_id, _url) => unsupportedCode(),
    webviewLoadHtml: (_id, _html) => unsupportedCode(),
    webviewLoadFile: (_id, _path) => unsupportedCode(),
    webviewClose: (_id) => {},
    webviewTick: (_id) => new TextEncoder().encode("1\n0\n"),
    webviewPoll: (_id) => unsupportedPoll(),
    webviewReply: (_id, _replyId, _body) => {},
    webviewReplyErr: (_id, _replyId, _message) => {},
    webviewReplyBytes: (_id, _replyId, _body) => {},
    webviewEmit: (_id, _channel, _body) => {},
    webviewEmitBytes: (_id, _channel, _body) => {},
    webviewEval: async (_id, _js) => unsupportedEval(),
    webviewGetWindow: (_id, _prop) => 0,
    webviewWindowOp: (_id, _prop, _a, _b) => 1,
    webviewGetString: (_id, _prop) => new Uint8Array(0),
    webviewSetString: (_id, _prop, _value) => 1,
    // system.desktop: dialogs cancel, clipboard reads are empty, writes and opens fail.
    desktopDialogStart: () => 0,
    desktopDialogPoll: (_handle) => new TextEncoder().encode("C"),
    clipboardGet: (_kind, _format) => new TextEncoder().encode("0"),
    clipboardSet: (_kind, _format, _data) => 1,
    clipboardHas: (_format) => 0,
    clipboardClear: () => {},
    shellOpen: (_target) => new TextEncoder().encode("unsupported on this host"),
  };
}
