// Which renderer is this engine actually using?
//
// Written because the Firefox build used about five times the CPU of the Chromium build to
// display the same page, and "it fell back to software rendering" is the first thing that
// would explain both that and its memory. A guess about this is worth nothing; the page can
// simply be asked.
//
// `WEBGL_debug_renderer_info` is deprecated and Firefox warns about it, but it is still the
// only way to see the real adapter string from content, and `RENDERER` alone reports a
// generic value that cannot tell a GPU from a software rasteriser.
(function () {
  var out = { hardware_concurrency: navigator.hardwareConcurrency, device_memory: navigator.deviceMemory || null };
  try {
    var canvas = document.createElement('canvas');
    var gl = canvas.getContext('webgl2') || canvas.getContext('webgl');
    if (!gl) { out.webgl = 'unavailable'; return Promise.resolve(JSON.stringify(out)); }
    out.webgl = gl.getParameter(gl.VERSION);
    out.renderer_generic = gl.getParameter(gl.RENDERER);
    var dbg = gl.getExtension('WEBGL_debug_renderer_info');
    if (dbg) {
      out.vendor = gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL);
      out.renderer = gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL);
    }
    // A software rasteriser names itself. These are the ones that matter on Windows.
    var r = String(out.renderer || out.renderer_generic || '');
    out.looks_software = /swiftshader|llvmpipe|software|basic render|microsoft basic/i.test(r);
  } catch (e) {
    out.webgl_error = String(e);
  }
  return Promise.resolve(JSON.stringify(out));
})()
