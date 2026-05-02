function segmentedSelect(btn, name, value) {
    var form = btn.closest('form');
    if (form) {
        var hidden = form.querySelector('input[type="hidden"][name="' + name + '"]');
        if (hidden) hidden.value = value;
    }
    var siblings = btn.parentElement.querySelectorAll('button');
    siblings.forEach(function(b) {
        b.className = 'seg-warm-btn';
        b.setAttribute('aria-pressed', 'false');
    });
    btn.className = 'seg-warm-btn seg-warm-btn-on';
    btn.setAttribute('aria-pressed', 'true');
    knobChanged();
}
function knobChanged() {
    var m = document.getElementById('knob-metrics');
    if (m) m.textContent = '';
}
function presetClicked(label) {
    var p = document.getElementById('knob-preset-label');
    if (p) p.textContent = label;
    knobChanged();
}
