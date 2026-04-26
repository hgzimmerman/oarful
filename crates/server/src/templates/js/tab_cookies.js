// Tab cookie helpers for the lineup editor.
// Master cookie "editor_tabs" holds {tabs, active, nextId}.
// Per-tab cookies "tab_{id}" hold gatherState() strings.

var TAB_COOKIE_PATH = '/solve/';

function _setCookie(name, value, path) {
    document.cookie = name + '=' + encodeURIComponent(value) + ';path=' + (path || TAB_COOKIE_PATH) + ';SameSite=Lax';
}

function _getCookie(name) {
    var match = document.cookie.match(new RegExp('(?:^|; )' + name + '=([^;]*)'));
    return match ? decodeURIComponent(match[1]) : null;
}

function _deleteCookie(name, path) {
    document.cookie = name + '=;path=' + (path || TAB_COOKIE_PATH) + ';expires=Thu, 01 Jan 1970 00:00:00 GMT';
}

function getEditorTabs() {
    var raw = _getCookie('editor_tabs');
    if (!raw) return { tabs: [{ id: 0, label: 'Lineup 1' }], active: 0, nextId: 1 };
    try { return JSON.parse(raw); } catch (e) {
        return { tabs: [{ id: 0, label: 'Lineup 1' }], active: 0, nextId: 1 };
    }
}

function setEditorTabs(obj) {
    _setCookie('editor_tabs', JSON.stringify(obj));
}

function getTabState(id) {
    return _getCookie('tab_' + id) || '';
}

function setTabState(id, state) {
    _setCookie('tab_' + id, state);
}

function removeTabState(id) {
    _deleteCookie('tab_' + id);
}

function _getPracticeId() {
    var el = document.getElementById('lineup-editor');
    if (el) return el.dataset.practiceId;
    // Fallback: extract from current URL (/solve/{id}...).
    var match = window.location.pathname.match(/\/solve\/(\d+)/);
    return match ? match[1] : null;
}

function _saveCurrentTabState() {
    var meta = getEditorTabs();
    var editor = document.querySelector('[x-data]');
    if (editor && editor.__x) {
        setTabState(meta.active, editor.__x.$data.gatherState());
    }
}

function _reloadSolvePage() {
    var practiceId = _getPracticeId();
    if (practiceId) {
        window.location.href = '/solve/' + practiceId;
    } else {
        window.location.reload();
    }
}

function switchTab(targetId) {
    _saveCurrentTabState();
    var meta = getEditorTabs();
    meta.active = targetId;
    setEditorTabs(meta);
    _reloadSolvePage();
}

function addTab() {
    _saveCurrentTabState();
    var meta = getEditorTabs();
    var newId = meta.nextId;
    meta.tabs.push({ id: newId, label: 'Lineup ' + (meta.tabs.length + 1) });
    meta.nextId = newId + 1;
    meta.active = newId;
    setEditorTabs(meta);
    setTabState(newId, '');
    _reloadSolvePage();
}

function removeTab(id) {
    var meta = getEditorTabs();
    if (meta.tabs.length <= 1) return; // never remove last tab
    meta.tabs = meta.tabs.filter(function(t) { return t.id !== id; });
    removeTabState(id);
    if (meta.active === id) {
        meta.active = meta.tabs[0].id;
    }
    setEditorTabs(meta);
    switchTab(meta.active);
}

function createTabFromSSE(label, seatParams) {
    var meta = getEditorTabs();
    var newId = meta.nextId;
    meta.tabs.push({ id: newId, label: label });
    meta.nextId = newId + 1;
    setEditorTabs(meta);
    setTabState(newId, seatParams);
    // Append a tab pill matching the server-rendered underline style.
    var bar = document.getElementById('tab-bar');
    if (bar) {
        var addBtn = bar.querySelector('[data-tab-add]');
        var pill = document.createElement('button');
        pill.className = 'tab-pill inline-flex items-center gap-1 px-3 text-sm font-medium transition border-b-2 border-transparent text-ink-3 hover:text-ink hover:border-rule';
        pill.dataset.tabId = newId;
        pill.onclick = function() { switchTab(newId); };
        pill.innerHTML = '<span class="tab-label">' + label + '</span>'
            + '<span class="tab-close text-ink-3 hover:text-red-600 ml-1 text-xs" onclick="event.stopPropagation(); removeTab(' + newId + ');">\u00d7</span>';
        if (addBtn) bar.insertBefore(pill, addBtn);
        else bar.appendChild(pill);
    }
}

// Clear all tab cookies (called on page unload or when navigating away from solve).
function clearAllTabCookies() {
    var meta = getEditorTabs();
    meta.tabs.forEach(function(t) { removeTabState(t.id); });
    _deleteCookie('editor_tabs');
}
