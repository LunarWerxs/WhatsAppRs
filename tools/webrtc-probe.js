// Can this engine make a WhatsApp voice or video call?
//
// Servo never could, and that is one of the reasons it was retired (DECISIONS.md #13), so
// it is the first thing to check on a replacement. A real call needs three separate things
// and any one of them missing kills it silently:
//
//   1. RTCPeerConnection, plus a real ICE gathering pass that produces candidates. An
//      engine can expose the object and still fail to gather.
//   2. A microphone, through getUserMedia, which needs the HOST to grant the permission:
//      in CEF that is CefPermissionHandler::OnRequestMediaAccessPermission, and without it
//      the promise rejects with NotAllowedError.
//   3. The codecs. This is the one that catches people out: a Chromium built without
//      proprietary codecs has no H.264, so video calls to phones fail while audio works.
//      Reported here rather than assumed either way.
//
// Resolves with a JSON string, so the same file works over CDP and over WebDriver BiDi.
(function () {
  function names(kind) {
    try {
      var caps = RTCRtpSender.getCapabilities(kind);
      if (!caps) return null;
      return caps.codecs.map(function (c) { return c.mimeType; })
        .filter(function (v, i, a) { return a.indexOf(v) === i; });
    } catch (e) { return null; }
  }

  var out = {
    has_rtcpeerconnection: typeof RTCPeerConnection !== 'undefined',
    has_getusermedia: !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia),
    has_getdisplaymedia: !!(navigator.mediaDevices && navigator.mediaDevices.getDisplayMedia),
    audio_codecs: names('audio'),
    video_codecs: names('video'),
    secure_context: window.isSecureContext
  };
  out.has_h264 = !!(out.video_codecs || []).some(function (m) { return /h264/i.test(m); });
  out.has_opus = !!(out.audio_codecs || []).some(function (m) { return /opus/i.test(m); });

  if (!out.has_rtcpeerconnection) {
    return Promise.resolve(JSON.stringify(out));
  }

  return new Promise(function (resolve) {
    var done = false;
    function finish() {
      if (done) return;
      done = true;
      resolve(JSON.stringify(out));
    }

    // ICE first: it exercises the network stack and the UDP sockets, which is where an
    // engine that merely *declares* WebRTC falls over.
    var pc = new RTCPeerConnection({ iceServers: [{ urls: 'stun:stun.l.google.com:19302' }] });
    var candidates = [];
    pc.onicecandidate = function (e) { if (e.candidate) candidates.push(e.candidate.candidate); };
    pc.createDataChannel('probe');
    pc.createOffer()
      .then(function (offer) {
        out.offer_has_audio_m_line = /m=audio/.test(offer.sdp || '');
        return pc.setLocalDescription(offer);
      })
      .catch(function (e) { out.offer_error = String(e); });

    setTimeout(function () {
      out.ice_candidates = candidates.length;
      out.ice_kinds = candidates.map(function (c) {
        var m = /typ (\w+)/.exec(c);
        return m ? m[1] : '?';
      }).filter(function (v, i, a) { return a.indexOf(v) === i; });
      try { pc.close(); } catch (e) {}

      // Then the microphone. Asked for last because the permission prompt is the part
      // the host has to answer, and a hung prompt should not hide the ICE result.
      if (!out.has_getusermedia) { finish(); return; }
      var settled = false;
      var guard = setTimeout(function () {
        if (settled) return;
        settled = true;
        out.mic = 'timed out waiting for the permission answer';
        finish();
      }, 8000);
      navigator.mediaDevices.getUserMedia({ audio: true })
        .then(function (stream) {
          if (settled) return;
          settled = true; clearTimeout(guard);
          out.mic = 'granted';
          out.mic_tracks = stream.getAudioTracks().map(function (t) { return t.label || '(unlabelled)'; });
          stream.getTracks().forEach(function (t) { t.stop(); });
          finish();
        })
        .catch(function (e) {
          if (settled) return;
          settled = true; clearTimeout(guard);
          out.mic = (e && e.name) ? e.name + ': ' + e.message : String(e);
          finish();
        });
    }, 5000);
  });
})()
