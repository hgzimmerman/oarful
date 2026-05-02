// Focus trap for modal dialogs.
// Usage: call trapFocus(modalElement) after appending a modal to the DOM.
// Call releaseFocus() or remove the modal to clean up.
(function() {
  var _trapEl = null;
  var _prevFocus = null;

  window.trapFocus = function(el) {
    _prevFocus = document.activeElement;
    _trapEl = el;
    el.addEventListener('keydown', _onKeyDown);
    // Focus the first focusable element inside the modal
    var first = _getFocusable(el)[0];
    if (first) first.focus();
  };

  window.releaseFocus = function() {
    if (_trapEl) {
      _trapEl.removeEventListener('keydown', _onKeyDown);
      _trapEl = null;
    }
    if (_prevFocus && _prevFocus.focus) _prevFocus.focus();
    _prevFocus = null;
  };

  function _onKeyDown(e) {
    if (e.key === 'Escape') {
      // Find and click the close button (aria-label="Close" or "Dismiss")
      var closeBtn = _trapEl.querySelector('[aria-label="Close"], [aria-label="Dismiss"], [aria-label^="Dismiss"]');
      if (closeBtn) { closeBtn.click(); return; }
      // Fallback: find the backdrop and click it
      var backdrop = _trapEl.previousElementSibling;
      if (backdrop) { backdrop.click(); return; }
    }
    if (e.key !== 'Tab') return;
    var focusable = _getFocusable(_trapEl);
    if (focusable.length === 0) return;
    var first = focusable[0];
    var last = focusable[focusable.length - 1];
    if (e.shiftKey) {
      if (document.activeElement === first) {
        e.preventDefault();
        last.focus();
      }
    } else {
      if (document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  function _getFocusable(el) {
    return Array.from(el.querySelectorAll(
      'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
    )).filter(function(e) { return e.offsetParent !== null; });
  }
})();
