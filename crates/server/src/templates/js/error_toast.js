// Show a toast on non-2xx HTMX responses. The server returns a
// plain-text message body alongside the status code; this listener
// extracts it and displays it in the fixed toast element.
document.addEventListener('htmx:beforeSwap', function(evt) {
    var status = evt.detail.xhr.status;
    if (status >= 400) {
        var msg = evt.detail.xhr.responseText || ('Request failed (' + status + ')');
        var toast = document.getElementById('error-toast');
        var msgEl = document.getElementById('error-toast-msg');
        if (toast && msgEl) {
            msgEl.textContent = msg;
            toast.classList.remove('hidden');
            clearTimeout(window._toastTimer);
            window._toastTimer = setTimeout(function() {
                toast.classList.add('hidden');
            }, 5000);
        }
    }
});
