(function () {
  if (window.Dream && window.Dream.__dream_webview) return;
  var pending = {};
  var nextId = 1;
  var listeners = {};
  var byteListeners = {};
  var pulling = false;
  // wry maps custom schemes to http://<scheme>.localhost on WebView2.
  var IPC = /Windows/.test(navigator.userAgent)
    ? "http://dream-ipc.localhost/"
    : "dream-ipc://localhost/";
  var decoder = new TextDecoder();

  function toU8(body) {
    if (body instanceof Uint8Array) return body;
    if (body instanceof ArrayBuffer) return new Uint8Array(body);
    if (ArrayBuffer.isView && ArrayBuffer.isView(body)) {
      return new Uint8Array(body.buffer, body.byteOffset, body.byteLength);
    }
    return new Uint8Array(0);
  }
  function post(obj) {
    if (window.ipc && window.ipc.postMessage) {
      window.ipc.postMessage(JSON.stringify(obj));
    }
  }
  function ipcFetch(kind, n, channel, body) {
    return fetch(IPC + kind + "/" + n + "/" + encodeURIComponent(String(channel)), {
      method: "POST",
      body: body,
    });
  }
  function deliverFrames(u8) {
    var view = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
    var off = 0;
    while (off + 8 <= u8.length) {
      var channelLen = view.getUint32(off, true);
      off += 4;
      var channel = decoder.decode(u8.subarray(off, off + channelLen));
      off += channelLen;
      var bodyLen = view.getUint32(off, true);
      off += 4;
      var body = u8.subarray(off, off + bodyLen);
      off += bodyLen;
      var list = byteListeners[channel] || [];
      for (var i = 0; i < list.length; i++) {
        try { list[i](body); } catch (e) { console.error(e); }
      }
    }
  }
  function pull() {
    ipcFetch("p", 0, "", new Uint8Array(0)).then(function (res) {
      if (res.status === 410) { pulling = false; return; }
      return res.arrayBuffer().then(function (buf) {
        if (buf.byteLength > 0) deliverFrames(new Uint8Array(buf));
        pull();
      });
    }).catch(function (e) {
      pulling = false;
      console.error(e);
    });
  }

  window.Dream = {
    __dream_webview: true,
    emit: function (channel, body) {
      post({ k: "e", c: String(channel), b: body == null ? "" : String(body) });
    },
    emitBytes: function (channel, body) {
      ipcFetch("e", 0, channel, toU8(body)).catch(function (e) { console.error(e); });
    },
    invoke: function (channel, body) {
      var id = nextId++;
      return new Promise(function (resolve, reject) {
        pending[id] = { resolve: resolve, reject: reject };
        post({ k: "i", id: id, c: String(channel), b: body == null ? "" : String(body) });
      });
    },
    invokeBytes: function (channel, body) {
      var id = nextId++;
      return ipcFetch("i", id, channel, toU8(body)).then(function (res) {
        if (!res.ok) {
          return res.text().then(function (message) {
            throw new Error(message || "invoke failed");
          });
        }
        return res.arrayBuffer().then(function (buf) { return new Uint8Array(buf); });
      });
    },
    on: function (channel, handler) {
      var c = String(channel);
      if (!listeners[c]) listeners[c] = [];
      listeners[c].push(handler);
    },
    onBytes: function (channel, handler) {
      var c = String(channel);
      if (!byteListeners[c]) byteListeners[c] = [];
      byteListeners[c].push(handler);
      if (!pulling) {
        pulling = true;
        pull();
      }
    },
    __dispatch: function (channel, body) {
      var list = listeners[String(channel)] || [];
      for (var i = 0; i < list.length; i++) {
        try { list[i](body); } catch (e) { console.error(e); }
      }
    },
    __resolve: function (id, body) {
      var p = pending[id];
      if (p) { delete pending[id]; p.resolve(body); }
    },
    __reject: function (id, message) {
      var p = pending[id];
      if (p) { delete pending[id]; p.reject(new Error(message || "invoke failed")); }
    }
  };
})();
