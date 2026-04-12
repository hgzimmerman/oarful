function segmentedSelect(btn, name, value) {
    var form = btn.closest('form');
    if (form) {
        var hidden = form.querySelector('input[type="hidden"][name="' + name + '"]');
        if (hidden) hidden.value = value;
    }
    var siblings = btn.parentElement.querySelectorAll('button');
    siblings.forEach(function(b) {
        b.className = 'px-3 py-2 text-slate-700 hover:bg-slate-100';
    });
    btn.className = 'px-3 py-2 font-semibold bg-slate-800 text-white';
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
