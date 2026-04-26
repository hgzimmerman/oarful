function lineupEditor() {
    return {
        selected: null,
        selectedBoat: null,

        // Gather current placement + pin state from DOM + knobs form.
        gatherState() {
            var root = this.$root;
            var params = [];
            root.querySelectorAll('[data-boat][data-seat][data-rower]').forEach(function(el) {
                if (el.dataset.boat === 'bench' || el.dataset.boat === 'sculling') return;
                if (el.dataset.rower) {
                    params.push('seat=' + el.dataset.rower + ':' + el.dataset.boat + ':' + el.dataset.seat);
                }
            });
            root.querySelectorAll('[data-editor-boat]').forEach(function(card) {
                if (card.dataset.hidden !== 'true') {
                    params.push('boat=' + card.dataset.editorBoat);
                }
            });
            var knobsForm = document.querySelector('form[hx-get]');
            if (knobsForm) {
                ['lock', 'pin', 'was_pin', 'walkon', 'no_show', 'boat_pin', 'boat_was_pin', 'boat_lock'].forEach(function(name) {
                    knobsForm.querySelectorAll('input[name="' + name + '"]').forEach(function(el) {
                        params.push(name + '=' + el.value);
                    });
                });
            }
            return params.join('&');
        },

        rerender(params) {
            // Persist active tab state to cookie before re-rendering.
            if (typeof setTabState === 'function') {
                var meta = getEditorTabs();
                setTabState(meta.active, params);
            }
            var editor = document.getElementById('lineup-editor');
            var url = (editor || this.$root).dataset.editorUrl + '?' + params;
            htmx.ajax('GET', url, {target: editor || this.$root, swap: 'outerHTML'});
        },

        select(key) {
            if (!this.selected) {
                this.selected = key;
            } else if (this.selected === key) {
                this.selected = null;
            } else {
                this.doSwap(this.selected, key);
            }
        },

        // Helper: remove all knobs form inputs matching name + value.
        _removeKnobInput(name, value) {
            var container = document.getElementById('editor-knob-state');
            if (!container) return;
            container.querySelectorAll('input[name="' + name + '"][value="' + value + '"]').forEach(function(el) {
                el.remove();
            });
        },

        // Helper: add a knobs form hidden input.
        _addKnobInput(name, value) {
            var container = document.getElementById('editor-knob-state');
            if (!container) return;
            var inp = document.createElement('input');
            inp.type = 'hidden'; inp.name = name; inp.value = value;
            container.appendChild(inp);
        },

        // Helper: mark a seat as dirty (pinned) after a manual move.
        // Preserves lock state — a locked rower stays locked after a swap.
        // Also pins the boat (if not already locked).
        _markDirty(el) {
            if (!el.dataset.rower || el.dataset.boat === 'bench' || el.dataset.boat === 'sculling') return;
            var key = el.dataset.rower + ':' + el.dataset.boat + ':' + el.dataset.seat;
            var container = document.getElementById('editor-knob-state');
            // Check if this rower had a lock at their previous position.
            // Lock keys are rower:boat:seat — match on rower ID prefix.
            var rowerId = el.dataset.rower;
            var hadLock = false;
            if (container) {
                container.querySelectorAll('input[name="lock"]').forEach(function(inp) {
                    if (inp.value.startsWith(rowerId + ':')) {
                        hadLock = true;
                        inp.remove();
                    }
                });
            }
            // Clear any existing pin/was_pin for this key.
            ['pin', 'was_pin'].forEach(function(n) { this._removeKnobInput(n, key); }.bind(this));
            if (hadLock) {
                this._addKnobInput('lock', key);
            } else {
                this._addKnobInput('pin', key);
            }
            // Also mark the boat as dirty (if not already locked).
            var boatId = el.dataset.boat;
            var isBoatLocked = container && container.querySelector('input[name="boat_lock"][value="' + boatId + '"]');
            if (!isBoatLocked) {
                ['boat_pin', 'boat_was_pin', 'boat_lock'].forEach(function(n) {
                    this._removeKnobInput(n, boatId);
                }.bind(this));
                this._addKnobInput('boat_pin', boatId);
            }
        },

        // Helper: clear all pin state for a seat before it changes.
        _clearSeatState(el) {
            if (!el.dataset.rower) return;
            var key = el.dataset.rower + ':' + el.dataset.boat + ':' + el.dataset.seat;
            ['lock', 'pin', 'was_pin'].forEach(function(n) { this._removeKnobInput(n, key); }.bind(this));
        },

        doSwap(a, b) {
            if (a === 'bench:empty' || b === 'bench:empty') {
                var seated = (a === 'bench:empty') ? b : a;
                return this.toBench(seated);
            }
            var root = this.$root;
            var elA = root.querySelector('[data-key="' + a + '"]');
            var elB = root.querySelector('[data-key="' + b + '"]');
            if (!elA || !elB) return;
            // Remember each ROWER's state BEFORE clearing (state follows the rower).
            // elA has rowerA, elB has rowerB. After swap, rowerA goes to elB, rowerB to elA.
            var rowerAState = elA.dataset.pinState || 'clean';
            var rowerBState = elB.dataset.pinState || 'clean';
            // Clear state for both seats before swap.
            this._clearSeatState(elA);
            this._clearSeatState(elB);
            // Swap rower IDs.
            var tmpRower = elA.dataset.rower;
            elA.dataset.rower = elB.dataset.rower;
            elB.dataset.rower = tmpRower;
            // Re-apply: state follows the rower to their new seat.
            // elA now holds rowerB → apply rowerBState.
            // elB now holds rowerA → apply rowerAState.
            var self = this;
            [[elA, rowerBState], [elB, rowerAState]].forEach(function(pair) {
                var el = pair[0], rowerState = pair[1];
                if (!el.dataset.rower || el.dataset.boat === 'bench' || el.dataset.boat === 'sculling') return;
                var key = el.dataset.rower + ':' + el.dataset.boat + ':' + el.dataset.seat;
                if (rowerState === 'locked') {
                    self._addKnobInput('lock', key);
                } else {
                    self._addKnobInput('pin', key);
                }
                // Pin the boat too (if not already locked).
                var boatId = el.dataset.boat;
                var container = document.getElementById('editor-knob-state');
                var isBoatLocked = container && container.querySelector('input[name="boat_lock"][value="' + boatId + '"]');
                if (!isBoatLocked) {
                    ['boat_pin', 'boat_was_pin'].forEach(function(n) {
                        self._removeKnobInput(n, boatId);
                    });
                    self._addKnobInput('boat_pin', boatId);
                }
            });
            this.selected = null;
            this.rerender(this.gatherState());
        },

        toBench(key) {
            var el = this.$root.querySelector('[data-key="' + key + '"]');
            if (!el || el.dataset.boat === 'bench' || el.dataset.boat === 'sculling') return;
            if (!el.dataset.rower) return;
            this._clearSeatState(el);
            el.dataset.rower = '';
            this.selected = null;
            this.rerender(this.gatherState());
        },

        // Boat pill click: when a boat is selected for transfer,
        // clicking a pill transfers rowers to that boat. Otherwise toggle.
        boatPillClick(boatId) {
            if (this.selectedBoat !== null && this.selectedBoat !== boatId) {
                // Transfer from selectedBoat → this pill's boat
                var params = this.gatherState();
                params += '&transfer=' + this.selectedBoat + ':' + boatId;
                this.selectedBoat = null;
                this.selected = null;
                this.rerender(params);
                return;
            }
            // Cancel selection if clicking the source boat's pill
            if (this.selectedBoat === boatId) {
                this.selectedBoat = null;
                return;
            }
            // No boat selected: normal toggle
            this.toggleBoat(boatId);
        },

        // Select a boat for transfer (called from boat card header).
        selectBoatForTransfer(boatId) {
            if (this.selectedBoat === boatId) {
                this.selectedBoat = null;
            } else {
                this.selectedBoat = boatId;
            }
        },

        toggleBoat(boatId) {
            var card = this.$root.querySelector('[data-editor-boat="' + boatId + '"]');
            if (!card) return;
            var isHidden = card.dataset.hidden === 'true';
            if (isHidden) {
                card.dataset.hidden = 'false';
            } else {
                var self = this;
                card.querySelectorAll('[data-rower]').forEach(function(row) {
                    self._clearSeatState(row);
                    row.dataset.rower = '';
                });
                card.dataset.hidden = 'true';
                // Clear boat pin state when deactivating.
                ['boat_pin', 'boat_was_pin', 'boat_lock'].forEach(function(n) {
                    self._removeKnobInput(n, String(boatId));
                });
            }
            this.selected = null;
            this.rerender(this.gatherState());
        },

        selectAllBoats() {
            this.$root.querySelectorAll('[data-editor-boat]').forEach(function(card) {
                card.dataset.hidden = 'false';
            });
            this.selected = null;
            this.rerender(this.gatherState());
        },

        deselectAllBoats() {
            var self = this;
            this.$root.querySelectorAll('[data-editor-boat]').forEach(function(card) {
                card.querySelectorAll('[data-rower]').forEach(function(row) {
                    self._clearSeatState(row);
                    row.dataset.rower = '';
                });
                card.dataset.hidden = 'true';
            });
            // Clear all boat pin state.
            var container = document.getElementById('editor-knob-state');
            if (container) {
                ['boat_pin', 'boat_was_pin', 'boat_lock'].forEach(function(n) {
                    container.querySelectorAll('input[name="' + n + '"]').forEach(function(el) { el.remove(); });
                });
            }
            this.selected = null;
            this.rerender(this.gatherState());
        },

        // Seat state machine: clean→locked, dirty→clean, was_pinned→locked, locked→clean.
        cycleSeatState(currentState, seatKey) {
            if (currentState === 'clean') {
                this._addKnobInput('lock', seatKey);
            } else if (currentState === 'dirty') {
                this._removeKnobInput('pin', seatKey);
            } else if (currentState === 'was_pinned') {
                this._removeKnobInput('was_pin', seatKey);
                this._addKnobInput('lock', seatKey);
            } else if (currentState === 'locked') {
                this._removeKnobInput('lock', seatKey);
            }
            this.rerender(this.gatherState());
        },

        // Boat state machine: clean→locked, dirty→clean, was_pinned→locked, locked→clean.
        cycleBoatState(currentState, boatId) {
            var bid = String(boatId);
            if (currentState === 'clean') {
                this._addKnobInput('boat_lock', bid);
            } else if (currentState === 'dirty') {
                this._removeKnobInput('boat_pin', bid);
            } else if (currentState === 'was_pinned') {
                this._removeKnobInput('boat_was_pin', bid);
                this._addKnobInput('boat_lock', bid);
            } else if (currentState === 'locked') {
                this._removeKnobInput('boat_lock', bid);
            }
            this.rerender(this.gatherState());
        }
    };
}

// After the primary SSE event swaps in the editor, save the new
// state to the active tab cookie so tab switching preserves it.
document.addEventListener('htmx:afterSettle', function(evt) {
    if (typeof getEditorTabs !== 'function') return;
    var target = evt.detail.target;
    // Check if this settle was for the primary SSE swap (the
    // sse-swap="primary" container's child).
    if (!target || !target.querySelector || !target.querySelector('#lineup-editor')) return;
    var editor = target.querySelector('[x-data]') || document.querySelector('[x-data]');
    if (editor && editor.__x) {
        var state = editor.__x.$data.gatherState();
        var meta = getEditorTabs();
        setTabState(meta.active, state);
    }
});

// Animate the generate button during SSE streaming.
// Also pre-creates empty alt tabs with a loading animation.
function startGenerating() {
    var btn = document.getElementById('generate-btn');
    if (!btn) return;
    btn.disabled = true;
    btn.classList.add('generating');
    var label = btn.querySelector('.generate-label');
    if (label) label.textContent = 'Generating\u2026';

    // Mark the active tab as pending (generating into it).
    var meta = getEditorTabs();
    var activePill = document.querySelector('#tab-bar [data-tab-id="' + meta.active + '"]');
    if (activePill) {
        activePill.classList.add('tab-pending');
        activePill.dataset.pending = 'true';
    }

    // Pre-create empty alt tabs based on the alts knob value.
    var altsInput = document.querySelector('input[name="alts"]');
    var alts = altsInput ? parseInt(altsInput.value, 10) : 0;
    if (alts > 0) {
        // Unhide close buttons on existing tabs (hidden when only one tab).
        document.querySelectorAll('#tab-bar .tab-close.hidden').forEach(function(el) {
            el.classList.remove('hidden');
        });
        var meta = getEditorTabs();
        for (var i = 1; i <= alts; i++) {
            var newId = meta.nextId;
            meta.tabs.push({ id: newId, label: 'Alt ' + i });
            meta.nextId = newId + 1;
            setTabState(newId, '');
            // Append a pending tab pill to the tab bar.
            var bar = document.getElementById('tab-bar');
            if (bar) {
                var addBtn = bar.querySelector('[data-tab-add]');
                var pill = document.createElement('button');
                pill.className = 'tab-pill inline-flex items-center gap-1 px-3 text-sm font-medium transition border-b-2 border-transparent text-ink-3 tab-pending';
                pill.dataset.tabId = newId;
                pill.dataset.pending = 'true';
                pill.disabled = true;
                pill.innerHTML = '<span class="tab-label">Alt ' + i + '</span>';
                if (addBtn) bar.insertBefore(pill, addBtn);
                else bar.appendChild(pill);
            }
        }
        setEditorTabs(meta);
    }
}
function stopGenerating(elapsed) {
    var btn = document.getElementById('generate-btn');
    if (!btn) return;
    btn.disabled = false;
    btn.classList.remove('generating');
    var label = btn.querySelector('.generate-label');
    if (label) label.textContent = label.dataset.originalText || 'Generate lineup';
    if (elapsed) {
        var el = document.getElementById('last-run-label');
        if (el) el.textContent = 'Last run: ' + elapsed;
    }
}
