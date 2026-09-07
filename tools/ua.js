(async () => {
  const r = { userAgent: navigator.userAgent, title: document.title };
  r.userAgentData = (navigator.userAgentData && navigator.userAgentData.brands) ? navigator.userAgentData.brands : null;
  r.vendor = navigator.vendor;
  r.loggedOut = !!document.querySelector('canvas');
  return JSON.stringify(r);
})()
