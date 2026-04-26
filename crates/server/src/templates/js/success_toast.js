// Show a success toast. Called from HTMX hx-on::after-request handlers.
function showSuccessToast(msg) {
    var toast = document.getElementById('success-toast');
    var msgEl = document.getElementById('success-toast-msg');
    if (toast && msgEl) {
        msgEl.textContent = msg;
        toast.classList.remove('hidden');
        clearTimeout(window._successToastTimer);
        window._successToastTimer = setTimeout(function() {
            toast.classList.add('hidden');
        }, 3000);
    }
}
