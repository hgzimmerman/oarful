function lineupEditor() {
    return {
        selected: null,
        selectedBoat: null,

        // Gather current placement + pin state from DOM + knobs form.
        gatherState() {
            var root = this.$root;
            var params = [];
            root.querySelectorAll('tr[data-boat][data-seat][data-rower]').forEach(function(el) {
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
            var url = this.$root.dataset.editorUrl + '?' + params;
            htmx.ajax('GET', url, {target: this.$root, swap: 'outerHTML'});
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
            var form = document.querySelector('form[hx-get]');
            if (!form) return;
            form.querySelectorAll('input[name="' + name + '"][value="' + value + '"]').forEach(function(el) {
                el.remove();
            });
        },

        // Helper: add a knobs form hidden input.
        _addKnobInput(name, value) {
            var form = document.querySelector('form[hx-get]');
            if (!form) return;
            var inp = document.createElement('input');
            inp.type = 'hidden'; inp.name = name; inp.value = value;
            form.appendChild(inp);
        },

        // Helper: mark a seat as dirty (pinned) after a manual move.
        // Preserves lock state — a locked rower stays locked after a swap.
        // Also pins the boat (if not already locked).
        _markDirty(el) {
            if (!el.dataset.rower || el.dataset.boat === 'bench' || el.dataset.boat === 'sculling') return;
            var key = el.dataset.rower + ':' + el.dataset.boat + ':' + el.dataset.seat;
            var form = document.querySelector('form[hx-get]');
            // Check if this rower had a lock at their previous position.
            // Lock keys are rower:boat:seat — match on rower ID prefix.
            var rowerId = el.dataset.rower;
            var hadLock = false;
            if (form) {
                form.querySelectorAll('input[name="lock"]').forEach(function(inp) {
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
            var form = document.querySelector('form[hx-get]');
            var isBoatLocked = form && form.querySelector('input[name="boat_lock"][value="' + boatId + '"]');
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
                var form = document.querySelector('form[hx-get]');
                var isBoatLocked = form && form.querySelector('input[name="boat_lock"][value="' + boatId + '"]');
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
                card.querySelectorAll('tr[data-rower]').forEach(function(row) {
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
                card.querySelectorAll('tr[data-rower]').forEach(function(row) {
                    self._clearSeatState(row);
                    row.dataset.rower = '';
                });
                card.dataset.hidden = 'true';
            });
            // Clear all boat pin state.
            var form = document.querySelector('form[hx-get]');
            if (form) {
                ['boat_pin', 'boat_was_pin', 'boat_lock'].forEach(function(n) {
                    form.querySelectorAll('input[name="' + n + '"]').forEach(function(el) { el.remove(); });
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
