// Tab cookie helpers for the lineup editor.
// Master cookie "editor_tabs" holds {tabs, active, nextId}.
// Per-tab cookies "tab_{id}" hold gatherState() strings.

var TAB_COOKIE_PATH = '/';

function _setCookie(name, value, path) {
    document.cookie = name + '=' + encodeURIComponent(value) + ';path=' + (path || TAB_COOKIE_PATH) + ';SameSite=Lax';
}

function _getCookie(name) {
    var match = document.cookie.match(new RegExp('(?:^|; )' + name + '=([^;]*)'));
    if (!match) return null;
    try { return decodeURIComponent(match[1]); } catch(e) { return match[1]; }
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
    if (practiceId && typeof htmx !== 'undefined') {
        htmx.ajax('GET', '/solve/' + practiceId, {
            target: '#content',
            swap: 'innerHTML'
        });
    } else if (practiceId) {
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

function removeTab(id) {
    var meta = getEditorTabs();
    if (meta.tabs.length <= 1) return;
    meta.tabs = meta.tabs.filter(function(t) { return t.id !== id; });
    removeTabState(id);
    if (meta.active === id) {
        meta.active = meta.tabs[0].id;
    }
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



function createTabFromSSE(label, seatParams) {
    var meta = getEditorTabs();
    // Look for the first pending tab pill in the DOM.
    var pill = document.querySelector('#tab-bar [data-pending="true"]');
    if (pill) {
        var tabId = parseInt(pill.dataset.tabId, 10);
        // Update the meta entry's label to match what the server sent.
        for (var i = 0; i < meta.tabs.length; i++) {
            if (meta.tabs[i].id === tabId) {
                meta.tabs[i].label = label;
                break;
            }
        }
        setEditorTabs(meta);
        setTabState(tabId, seatParams);
        // Activate the pill.
        pill.removeAttribute('data-pending');
        pill.classList.remove('tab-pending');
        pill.disabled = false;
        pill.onclick = function() { switchTab(tabId); };
        pill.innerHTML = '<span class="tab-label">' + label + '</span>'
            + '<span class="tab-close text-ink-3 hover:text-red-600 ml-1 text-xs" onclick="event.stopPropagation(); removeTab(' + tabId + ');">\u00d7</span>';
        pill.classList.add('hover:text-ink', 'hover:border-rule');
    } else {
        // Fallback: create a new tab if no pending match found.
        var newId = meta.nextId;
        meta.tabs.push({ id: newId, label: label });
        meta.nextId = newId + 1;
        setEditorTabs(meta);
        setTabState(newId, seatParams);
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
}

// Clear all tab cookies when navigating away from the solve page.
function clearAllTabCookies() {
    var meta = getEditorTabs();
    meta.tabs.forEach(function(t) { removeTabState(t.id); });
    _deleteCookie('editor_tabs');
}

// Clear stale tab cookies when navigating away from a solve page.
// Listen on click for any link/element that navigates to a non-solve path.
document.addEventListener('click', function(evt) {
    if (!window.location.pathname.startsWith('/solve/')) return;
    var link = evt.target.closest('a[href], [hx-get], [hx-post]');
    if (!link) return;
    var dest = link.getAttribute('href') || link.getAttribute('hx-get') || link.getAttribute('hx-post') || '';
    if (dest && !dest.startsWith('/solve/')) {
        clearAllTabCookies();
    }
});
