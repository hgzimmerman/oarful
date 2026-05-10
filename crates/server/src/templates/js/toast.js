// Unified toast. Usage:
//   showToast('message')              — neutral
//   showToast('message', 'success')   — green
//   showToast('message', 'error')     — red
//
// Auto-hides after 4s (error: 6s). Clicking x dismisses immediately.

var _toastStyles = {
    success: 'bg-emerald-50 text-emerald-900 border-emerald-300 border-l-emerald-600',
    error:   'bg-red-50 text-red-900 border-red-300 border-l-red-600',
    neutral: 'bg-paper text-ink border-rule border-l-ink-3'
};

function _dismissToast() {
    var toast = document.getElementById('toast');
    if (!toast || toast.classList.contains('hidden')) return;
    toast.classList.remove('toast-enter');
    toast.classList.add('toast-exit');
    toast.addEventListener('animationend', function handler() {
        toast.classList.add('hidden');
        toast.classList.remove('toast-exit');
        toast.removeEventListener('animationend', handler);
    }, { once: true });
}

function showToast(msg, type) {
    var toast = document.getElementById('toast');
    var inner = document.getElementById('toast-inner');
    var msgEl = document.getElementById('toast-msg');
    if (!toast || !inner || !msgEl) return;
    msgEl.textContent = msg;
    // Reset style classes, apply the right variant.
    var styles = _toastStyles[type] || _toastStyles.neutral;
    inner.className = 'px-4 py-3 rounded-lg shadow-lg flex items-start gap-3 text-sm border border-l-4 ' + styles;
    toast.classList.remove('hidden', 'toast-exit');
    toast.classList.add('toast-enter');
    clearTimeout(window._toastTimer);
    window._toastTimer = setTimeout(_dismissToast, type === 'error' ? 6000 : 4000);
}

// Convenience wrappers.
function showSuccessToast(msg) { showToast(msg, 'success'); }
function showErrorToast(msg) { showToast(msg, 'error'); }

// Auto-show error toast on non-2xx HTMX responses.
document.addEventListener('htmx:beforeSwap', function(evt) {
    var status = evt.detail.xhr.status;
    if (status >= 400) {
        var msg = evt.detail.xhr.responseText || ('Request failed (' + status + ')');
        showErrorToast(msg);
    }
});
