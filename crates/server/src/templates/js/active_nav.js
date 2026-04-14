function updateActiveNav() {
    var path = location.pathname;
    // Map related paths to a single nav section.
    var aliases = {
        '/practices': ['/history', '/solve', '/commit'],
        '/team': ['/rowers', '/sync'],
        '/admin': ['/users', '/teams', '/audit', '/boats']
    };
    document.querySelectorAll('[data-nav]').forEach(function(a) {
        var href = a.dataset.nav;
        var active = (href === '/' && path === '/') ||
                     (href !== '/' && path.startsWith(href));
        if (!active && aliases[href]) {
            active = aliases[href].some(function(p) { return path.startsWith(p); });
        }
        if (active) {
            a.classList.add('bg-white/15', 'font-semibold');
        } else {
            a.classList.remove('bg-white/15', 'font-semibold');
        }
    });
}
document.addEventListener('DOMContentLoaded', updateActiveNav);
document.addEventListener('htmx:pushedIntoHistory', updateActiveNav);
