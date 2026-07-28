/// BlissLang Signal System — v0.3
///
/// Generates the JavaScript signal/reactivity engine that is embedded
/// into every BlissLang page via /_bliss/runtime.js
///
/// The signal system is:
///   - Pure vanilla JS, no framework dependency
///   - ~3KB before minification
///   - Built around a publish/subscribe model with dependency tracking
///   - Wires directly to data-reactive, data-showif, data-foreach elements
///
/// Architecture:
///   Signal         — a reactive value container
///   Derived        — a computed value that updates when its sources update
///   Effect         — a side effect that runs when its watched signals update
///   SignalBridge   — connects Rust-rendered HTML attributes to the signal system

pub fn signal_js() -> &'static str {
    r#"
// ══════════════════════════════════════════════════════════════════════════════
// BlissLang Signal System v0.3
// Reactive state — zero framework, pure JS
// ══════════════════════════════════════════════════════════════════════════════
(function(bliss) {
    'use strict';

    // ── Internal subscriber tracking ────────────────────────────────────────
    var _current_effect = null;   // the effect currently being evaluated
    var _batch_queue    = [];     // pending updates when batching
    var _batching       = false;  // are we inside a batch?

    // ── Signal ──────────────────────────────────────────────────────────────
    //
    // A Signal holds a value and notifies subscribers when it changes.
    //
    // Usage:
    //   var count = bliss.signal(0);
    //   count.get()          // → 0
    //   count.set(1)         // notifies subscribers
    //   count.update(n => n + 1)

    function signal(initial) {
        var _value       = initial;
        var _subscribers = new Set();

        function track() {
            // If an effect is currently running, subscribe it to this signal
            if (_current_effect) {
                _subscribers.add(_current_effect);
                _current_effect._sources.add(notify);
            }
        }

        function notify() {
            if (_batching) {
                _batch_queue.push(function() { _notify(); });
                return;
            }
            _notify();
        }

        function _notify() {
            // Copy subscribers before iterating — a subscriber might
            // unsubscribe itself during notification
            var subs = Array.from(_subscribers);
            subs.forEach(function(sub) { sub(); });
        }

        return {
            get: function() {
                track();
                return _value;
            },
            set: function(newVal) {
                if (_value === newVal) return; // no-op if unchanged
                _value = newVal;
                notify();
            },
            update: function(fn) {
                this.set(fn(_value));
            },
            peek: function() {
                // Read without tracking (no subscription created)
                return _value;
            },
            subscribe: function(fn) {
                _subscribers.add(fn);
                return function() { _subscribers.delete(fn); }; // unsubscribe
            },
            _subscribers: _subscribers,
            _type: 'signal'
        };
    }

    // ── Derived ─────────────────────────────────────────────────────────────
    //
    // A Derived is a read-only computed value.
    // It automatically tracks which signals it reads and recomputes when any change.
    //
    // Usage:
    //   var cartTotal = bliss.derived(function() {
    //       return cart.get().reduce(function(sum, item) { return sum + item.price; }, 0);
    //   });

    function derived(computeFn) {
        var _cached      = undefined;
        var _dirty       = true;
        var _subscribers = new Set();

        var self = {
            get: function() {
                if (_dirty) {
                    // Re-run compute, tracking dependencies
                    var prev = _current_effect;
                    _current_effect = recompute;
                    recompute._sources = new Set();
                    try {
                        _cached = computeFn();
                        _dirty  = false;
                    } finally {
                        _current_effect = prev;
                    }
                } else {
                    // Still track for downstream effects
                    if (_current_effect) {
                        _subscribers.add(_current_effect);
                    }
                }
                return _cached;
            },
            subscribe: function(fn) {
                _subscribers.add(fn);
                return function() { _subscribers.delete(fn); };
            },
            _type: 'derived'
        };

        function recompute() {
            _dirty = true;
            var subs = Array.from(_subscribers);
            subs.forEach(function(sub) { sub(); });
        }
        recompute._sources = new Set();

        return self;
    }

    // ── Effect ──────────────────────────────────────────────────────────────
    //
    // An Effect runs a function whenever its tracked signals change.
    // It automatically discovers dependencies by running the function once.
    //
    // Usage:
    //   bliss.effect(function() {
    //       document.title = 'Cart: ' + cartCount.get();
    //   });

    function effect(fn) {
        var _sources = new Set();

        function run() {
            // Clean up old subscriptions
            _sources.forEach(function(unsub) { unsub(); });
            _sources.clear();

            var prev = _current_effect;
            _current_effect = run;
            run._sources    = _sources;

            try {
                fn();
            } finally {
                _current_effect = prev;
            }
        }

        run._sources = _sources;
        run();  // run immediately to discover dependencies

        return {
            dispose: function() {
                _sources.forEach(function(unsub) { unsub(); });
                _sources.clear();
            }
        };
    }

    // ── Batch ───────────────────────────────────────────────────────────────
    //
    // Run multiple signal updates without triggering intermediate renders.
    //
    // Usage:
    //   bliss.batch(function() {
    //       user.set(newUser);
    //       isLoggedIn.set(true);
    //       cart.set([]);
    //   });
    //   // DOM updates once after all three signals change

    function batch(fn) {
        _batching = true;
        try {
            fn();
        } finally {
            _batching = false;
            var queue = _batch_queue.slice();
            _batch_queue = [];
            // Deduplicate and run
            var seen = new Set();
            queue.forEach(function(job) {
                if (!seen.has(job)) {
                    seen.add(job);
                    job();
                }
            });
        }
    }

    // ── DOM Binding ─────────────────────────────────────────────────────────
    //
    // Connects signals to rendered HTML elements via data attributes.
    //
    // Supported bindings:
    //   data-reactive="signal.path"    — updates textContent or value
    //   data-showif="signal.path"      — show/hide element
    //   data-bind-class="signal.path"  — toggle CSS class
    //   data-bind-attr="attr:signal"   — bind any HTML attribute

    function bindDOM() {
        // ── data-reactive ──────────────────────────────────────────────────
        var reactEls = document.querySelectorAll('[data-reactive]');
        reactEls.forEach(function(el) {
            var path = el.dataset.reactive;
            // Parse array notation: ["signal1", "signal2"]
            var signals = path.replace(/[\[\]"'\s]/g, '').split(',').filter(Boolean);

            signals.forEach(function(sigPath) {
                var sig = resolvePath(sigPath);
                if (!sig) return;

                effect(function() {
                    var val = typeof sig.get === 'function' ? sig.get() : sig;
                    updateElement(el, val);
                });
            });
        });

        // ── data-showif ───────────────────────────────────────────────────
        var showEls = document.querySelectorAll('[data-showif]');
        showEls.forEach(function(el) {
            var expr = el.dataset.showif;

            effect(function() {
                var result = evaluateExpr(expr);
                el.style.display = result ? '' : 'none';
            });
        });

        // ── data-showelse ──────────────────────────────────────────────────
        var elseEls = document.querySelectorAll('[data-showelse]');
        elseEls.forEach(function(el) {
            var expr = el.dataset.showelse;

            effect(function() {
                var result = evaluateExpr(expr);
                el.style.display = result ? 'none' : '';
            });
        });

        // ── data-bind-class ────────────────────────────────────────────────
        var classEls = document.querySelectorAll('[data-bind-class]');
        classEls.forEach(function(el) {
            var spec = el.dataset.bindClass; // "active:isActive,hidden:!isVisible"
            spec.split(',').forEach(function(pair) {
                var parts    = pair.trim().split(':');
                var cssClass = parts[0].trim();
                var sigPath  = parts[1].trim();

                effect(function() {
                    var val = evaluateExpr(sigPath);
                    el.classList.toggle(cssClass, !!val);
                });
            });
        });

        // ── data-foreach ───────────────────────────────────────────────────
        var foreachEls = document.querySelectorAll('[data-foreach]');
        foreachEls.forEach(function(container) {
            var sigPath  = container.dataset.foreach;
            var template = container.querySelector('[data-foreach-item]');
            if (!template) return;

            var sig = resolvePath(sigPath);
            if (!sig) return;

            effect(function() {
                var items = typeof sig.get === 'function' ? sig.get() : [];
                if (!Array.isArray(items)) return;

                // Clear existing items (keep template)
                var existing = container.querySelectorAll('[data-foreach-rendered]');
                existing.forEach(function(el) { el.remove(); });

                // Render each item by cloning the template
                items.forEach(function(item, index) {
                    var clone = template.cloneNode(true);
                    clone.removeAttribute('data-foreach-item');
                    clone.setAttribute('data-foreach-rendered', index);
                    clone.style.display = '';

                    // Replace {{field}} placeholders in the clone
                    interpolateNode(clone, item, index);
                    container.appendChild(clone);
                });
            });

            // Hide the template
            template.style.display = 'none';
        });
    }

    // ── Element Update ─────────────────────────────────────────────────────

    function updateElement(el, val) {
        var tag = el.tagName.toLowerCase();

        if (tag === 'input' || tag === 'textarea' || tag === 'select') {
            if (el.type === 'checkbox' || el.type === 'radio') {
                el.checked = !!val;
            } else {
                el.value = val == null ? '' : String(val);
            }
        } else if (tag === 'img') {
            el.src = val == null ? '' : String(val);
        } else {
            el.textContent = val == null ? '' : String(val);
        }
    }

    // ── Interpolation ───────────────────────────────────────────────────────

    function interpolateNode(node, item, index) {
        // Walk all text nodes and replace {{field}} with item values
        var walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
        var textNodes = [];
        while (walker.nextNode()) { textNodes.push(walker.currentNode); }

        textNodes.forEach(function(tn) {
            tn.textContent = tn.textContent.replace(/\{\{(\w+)\}\}/g, function(_, key) {
                if (key === '__index') return index;
                return item[key] != null ? String(item[key]) : '';
            });
        });

        // Also replace in key attributes
        var attrs = ['href', 'src', 'alt', 'title', 'data-id', 'value'];
        node.querySelectorAll('*').forEach(function(el) {
            attrs.forEach(function(attr) {
                if (el.hasAttribute(attr)) {
                    el.setAttribute(attr, el.getAttribute(attr).replace(
                        /\{\{(\w+)\}\}/g,
                        function(_, key) { return item[key] != null ? String(item[key]) : ''; }
                    ));
                }
            });
        });
    }

    // ── Expression Evaluator ────────────────────────────────────────────────
    //
    // Evaluates simple BlissLang expressions against the signal store.
    // Handles: signal paths, negation, comparisons, null coalescing.

    function evaluateExpr(expr) {
        expr = expr.trim();

        // Negation
        if (expr.startsWith('!')) {
            return !evaluateExpr(expr.slice(1));
        }

        // Null coalescing: a ?? b
        if (expr.includes('??')) {
            var parts = expr.split('??').map(function(p) { return p.trim(); });
            var left  = evaluateExpr(parts[0]);
            return left != null ? left : evaluateExpr(parts.slice(1).join('??'));
        }

        // Comparison operators
        var ops = [' == ', ' != ', ' >= ', ' <= ', ' > ', ' < ', ' && ', ' || '];
        for (var i = 0; i < ops.length; i++) {
            var op = ops[i];
            var idx = expr.indexOf(op);
            if (idx !== -1) {
                var left  = evaluateExpr(expr.slice(0, idx));
                var right = evaluateExpr(expr.slice(idx + op.length));
                switch (op.trim()) {
                    case '==':  return left == right;
                    case '!=':  return left != right;
                    case '>=':  return left >= right;
                    case '<=':  return left <= right;
                    case '>':   return left >  right;
                    case '<':   return left <  right;
                    case '&&':  return left && right;
                    case '||':  return left || right;
                }
            }
        }

        // String literals
        if ((expr.startsWith('"') && expr.endsWith('"')) ||
            (expr.startsWith("'") && expr.endsWith("'"))) {
            return expr.slice(1, -1);
        }

        // Number literals
        if (!isNaN(Number(expr))) return Number(expr);

        // Boolean literals
        if (expr === 'true')  return true;
        if (expr === 'false') return false;
        if (expr === 'null')  return null;

        // Signal path: e.g. App.cartCount, State.user.name
        var sig = resolvePath(expr);
        if (sig != null) {
            return typeof sig.get === 'function' ? sig.get() : sig;
        }

        return undefined;
    }

    // ── Path Resolver ───────────────────────────────────────────────────────
    //
    // Resolves "App.cartCount" → bliss.state.App.cartCount.get()

    function resolvePath(path) {
        var parts = path.trim().split('.');
        var obj   = bliss.state;

        for (var i = 0; i < parts.length; i++) {
            if (obj == null) return null;
            obj = obj[parts[i]];
        }

        return obj;
    }

    // ── Two-Way Input Binding ────────────────────────────────────────────────

    function bindInputs() {
        var inputs = document.querySelectorAll('[data-model]');
        inputs.forEach(function(el) {
            var path = el.dataset.model;
            var sig  = resolvePath(path);
            if (!sig || typeof sig.set !== 'function') return;

            // Set initial value
            el.value = sig.get() != null ? String(sig.get()) : '';

            // Update signal on input
            el.addEventListener('input', function() {
                var val = el.type === 'checkbox' ? el.checked :
                          el.type === 'number'   ? Number(el.value) :
                          el.value;
                sig.set(val);
            });

            // Update element when signal changes
            effect(function() {
                var val = sig.get();
                if (el.type === 'checkbox') {
                    el.checked = !!val;
                } else {
                    el.value = val != null ? String(val) : '';
                }
            });
        });
    }

    // ── Event Handler Binding ────────────────────────────────────────────────

    function bindEventHandlers() {
        // data-onclick="handlerName" — wire to bliss.handlers
        var clickEls = document.querySelectorAll('[data-onclick]');
        clickEls.forEach(function(el) {
            var handler = el.dataset.onclick;
            el.addEventListener('click', function(e) {
                var fn = bliss.handlers[handler];
                if (typeof fn === 'function') fn(e);
                else console.warn('[BlissLang] No handler registered:', handler);
            });
        });

        // data-onsubmit — form submission
        var forms = document.querySelectorAll('[data-onsubmit]');
        forms.forEach(function(form) {
            var handler = form.dataset.onsubmit;
            form.addEventListener('submit', function(e) {
                e.preventDefault();
                var fn = bliss.handlers[handler];
                if (typeof fn === 'function') {
                    // Collect form data
                    var data = {};
                    new FormData(form).forEach(function(val, key) { data[key] = val; });
                    fn(data, e);
                }
            });
        });
    }

    // ── Navigate ─────────────────────────────────────────────────────────────

    function navigate(url) {
        window.location.href = url;
    }

    // ── Toast Notifications ──────────────────────────────────────────────────

    function showToast(message, type) {
        type = type || 'info';
        var toast    = document.createElement('div');
        var colors   = {
            info:    'bg-blue-600',
            success: 'bg-green-600',
            error:   'bg-red-600',
            warning: 'bg-yellow-600'
        };
        toast.className = 'fixed bottom-4 right-4 px-6 py-3 text-white rounded-lg shadow-lg z-50 ' +
                          (colors[type] || colors.info);
        toast.textContent = message;
        document.body.appendChild(toast);
        setTimeout(function() {
            toast.style.opacity = '0';
            toast.style.transition = 'opacity 0.3s';
            setTimeout(function() { toast.remove(); }, 300);
        }, 3000);
    }

    // ── Public API ──────────────────────────────────────────────────────────

    bliss.signal    = signal;
    bliss.derived   = derived;
    bliss.effect    = effect;
    bliss.batch     = batch;
    bliss.navigate  = navigate;
    bliss.showToast = showToast;
    bliss.state     = {};        // all CreateState[] blocks register here
    bliss.handlers  = {};        // event handlers register here

    // ── Init ─────────────────────────────────────────────────────────────────

    bliss._initDOM = function() {
        bindDOM();
        bindInputs();
        bindEventHandlers();
        console.log('%c BlissLang Signals %c ready — ' + Object.keys(bliss.state).length + ' state trees ',
            'background:#0F3460;color:#fff;padding:2px 6px;border-radius:3px 0 0 3px;font-weight:bold',
            'background:#1A1A2E;color:#A8B2C1;padding:2px 6px;border-radius:0 3px 3px 0'
        );
    };

})(window.__bliss);
"#
}

/// Generate a state initialisation JS block for a CreateState node.
/// Called by the renderer when it encounters a StateNode.
#[allow(dead_code)]
pub fn state_init_js(name: &str, signals: &[(&str, &str, &str)]) -> String {
    // signals: Vec of (signal_name, type_hint, default_value_js)
    let mut js = format!("// State: {}\n(function() {{\n    var s = {{}};\n", name);

    for (sig_name, _ty, default) in signals {
        js.push_str(&format!(
            "    s.{} = window.__bliss.signal({});\n",
            sig_name, default
        ));
    }

    js.push_str(&format!(
        "    window.__bliss.state.{} = s;\n}})();\n",
        name
    ));

    js
}

/// Generate a derived signal initialisation block.
#[allow(dead_code)]
pub fn derived_init_js(state_name: &str, derived_name: &str, compute_expr: &str) -> String {
    format!(
        r#"(function() {{
    window.__bliss.state.{state}.{name} = window.__bliss.derived(function() {{
        var s = window.__bliss.state.{state};
        return {expr};
    }});
}})();
"#,
        state = state_name,
        name  = derived_name,
        expr  = compute_expr
    )
}
