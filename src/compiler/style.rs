/// BlissLang Style Compiler — v0.4
///
/// Eliminates the Tailwind CDN dependency.
/// Scans all Tailwind classes used in the project and generates
/// a self-contained CSS bundle — zero external network calls.
///
/// Strategy:
///   1. Walk every rendered HTML string and collect class="" values
///   2. For each class, look it up in the built-in utility table
///   3. Write a purged, minimal CSS file with only used classes
///
/// This covers the ~150 most common Tailwind utilities.
/// Full Tailwind JIT compilation will come in v0.5 via a Rust-native parser.

use std::collections::HashSet;

// ─── CSS class registry ───────────────────────────────────────────────────────

/// Returns the CSS rule for a given Tailwind class name.
/// Returns None if the class is not recognised (it will be silently skipped).
pub fn lookup(class: &str) -> Option<&'static str> {
    Some(match class {
        // ── Display ──────────────────────────────────────────────────────
        "block"        => "display:block",
        "inline"       => "display:inline",
        "inline-block" => "display:inline-block",
        "flex"         => "display:flex",
        "inline-flex"  => "display:inline-flex",
        "grid"         => "display:grid",
        "inline-grid"  => "display:inline-grid",
        "hidden"       => "display:none",
        "contents"     => "display:contents",
        "table"        => "display:table",

        // ── Position ─────────────────────────────────────────────────────
        "static"   => "position:static",
        "fixed"    => "position:fixed",
        "absolute" => "position:absolute",
        "relative" => "position:relative",
        "sticky"   => "position:sticky",

        // ── Inset ────────────────────────────────────────────────────────
        "inset-0"    => "inset:0",
        "inset-x-0"  => "left:0;right:0",
        "inset-y-0"  => "top:0;bottom:0",
        "top-0"      => "top:0",
        "right-0"    => "right:0",
        "bottom-0"   => "bottom:0",
        "left-0"     => "left:0",
        "top-4"      => "top:1rem",
        "right-4"    => "right:1rem",
        "bottom-4"   => "bottom:1rem",
        "left-4"     => "left:1rem",
        "-top-1"     => "top:-0.25rem",
        "-right-1"   => "right:-0.25rem",

        // ── Flexbox ──────────────────────────────────────────────────────
        "flex-col"          => "flex-direction:column",
        "flex-row"          => "flex-direction:row",
        "flex-wrap"         => "flex-wrap:wrap",
        "flex-nowrap"       => "flex-wrap:nowrap",
        "flex-1"            => "flex:1 1 0%",
        "flex-auto"         => "flex:1 1 auto",
        "flex-none"         => "flex:none",
        "flex-shrink-0"     => "flex-shrink:0",
        "flex-grow"         => "flex-grow:1",
        "items-start"       => "align-items:flex-start",
        "items-center"      => "align-items:center",
        "items-end"         => "align-items:flex-end",
        "items-stretch"     => "align-items:stretch",
        "items-baseline"    => "align-items:baseline",
        "justify-start"     => "justify-content:flex-start",
        "justify-center"    => "justify-content:center",
        "justify-end"       => "justify-content:flex-end",
        "justify-between"   => "justify-content:space-between",
        "justify-around"    => "justify-content:space-around",
        "justify-evenly"    => "justify-content:space-evenly",
        "self-start"        => "align-self:flex-start",
        "self-center"       => "align-self:center",
        "self-end"          => "align-self:flex-end",

        // ── Grid ─────────────────────────────────────────────────────────
        "grid-cols-1"  => "grid-template-columns:repeat(1,minmax(0,1fr))",
        "grid-cols-2"  => "grid-template-columns:repeat(2,minmax(0,1fr))",
        "grid-cols-3"  => "grid-template-columns:repeat(3,minmax(0,1fr))",
        "grid-cols-4"  => "grid-template-columns:repeat(4,minmax(0,1fr))",
        "grid-cols-5"  => "grid-template-columns:repeat(5,minmax(0,1fr))",
        "grid-cols-6"  => "grid-template-columns:repeat(6,minmax(0,1fr))",
        "col-span-1"   => "grid-column:span 1/span 1",
        "col-span-2"   => "grid-column:span 2/span 2",
        "col-span-3"   => "grid-column:span 3/span 3",
        "col-span-full"=> "grid-column:1/-1",
        "col-start-1"  => "grid-column-start:1",
        "col-start-2"  => "grid-column-start:2",

        // ── Sizing ───────────────────────────────────────────────────────
        "w-full"       => "width:100%",
        "w-screen"     => "width:100vw",
        "w-auto"       => "width:auto",
        "w-fit"        => "width:fit-content",
        "w-0"          => "width:0",
        "w-4"          => "width:1rem",
        "w-5"          => "width:1.25rem",
        "w-6"          => "width:1.5rem",
        "w-8"          => "width:2rem",
        "w-10"         => "width:2.5rem",
        "w-12"         => "width:3rem",
        "w-16"         => "width:4rem",
        "w-20"         => "width:5rem",
        "w-24"         => "width:6rem",
        "w-32"         => "width:8rem",
        "w-48"         => "width:12rem",
        "w-64"         => "width:16rem",
        "w-96"         => "width:24rem",
        "w-1\\/2"      => "width:50%",
        "w-1\\/3"      => "width:33.333%",
        "w-2\\/3"      => "width:66.667%",
        "h-full"       => "height:100%",
        "h-screen"     => "height:100vh",
        "h-auto"       => "height:auto",
        "h-fit"        => "height:fit-content",
        "h-0"          => "height:0",
        "h-4"          => "height:1rem",
        "h-5"          => "height:1.25rem",
        "h-6"          => "height:1.5rem",
        "h-8"          => "height:2rem",
        "h-10"         => "height:2.5rem",
        "h-12"         => "height:3rem",
        "h-16"         => "height:4rem",
        "h-20"         => "height:5rem",
        "h-24"         => "height:6rem",
        "h-32"         => "height:8rem",
        "h-48"         => "height:12rem",
        "h-64"         => "height:16rem",
        "h-96"         => "height:24rem",
        "min-h-screen" => "min-height:100vh",
        "min-h-full"   => "min-height:100%",
        "min-h-0"      => "min-height:0",
        "max-w-sm"     => "max-width:24rem",
        "max-w-md"     => "max-width:28rem",
        "max-w-lg"     => "max-width:32rem",
        "max-w-xl"     => "max-width:36rem",
        "max-w-2xl"    => "max-width:42rem",
        "max-w-3xl"    => "max-width:48rem",
        "max-w-4xl"    => "max-width:56rem",
        "max-w-5xl"    => "max-width:64rem",
        "max-w-6xl"    => "max-width:72rem",
        "max-w-7xl"    => "max-width:80rem",
        "max-w-full"   => "max-width:100%",

        // ── Spacing — Padding ────────────────────────────────────────────
        "p-0"  => "padding:0",
        "p-1"  => "padding:0.25rem",
        "p-2"  => "padding:0.5rem",
        "p-3"  => "padding:0.75rem",
        "p-4"  => "padding:1rem",
        "p-5"  => "padding:1.25rem",
        "p-6"  => "padding:1.5rem",
        "p-8"  => "padding:2rem",
        "p-10" => "padding:2.5rem",
        "p-12" => "padding:3rem",
        "px-0" => "padding-left:0;padding-right:0",
        "px-1" => "padding-left:0.25rem;padding-right:0.25rem",
        "px-2" => "padding-left:0.5rem;padding-right:0.5rem",
        "px-3" => "padding-left:0.75rem;padding-right:0.75rem",
        "px-4" => "padding-left:1rem;padding-right:1rem",
        "px-6" => "padding-left:1.5rem;padding-right:1.5rem",
        "px-8" => "padding-left:2rem;padding-right:2rem",
        "py-0" => "padding-top:0;padding-bottom:0",
        "py-1" => "padding-top:0.25rem;padding-bottom:0.25rem",
        "py-2" => "padding-top:0.5rem;padding-bottom:0.5rem",
        "py-3" => "padding-top:0.75rem;padding-bottom:0.75rem",
        "py-4" => "padding-top:1rem;padding-bottom:1rem",
        "py-6" => "padding-top:1.5rem;padding-bottom:1.5rem",
        "py-8" => "padding-top:2rem;padding-bottom:2rem",
        "py-10"=> "padding-top:2.5rem;padding-bottom:2.5rem",
        "py-12"=> "padding-top:3rem;padding-bottom:3rem",
        "py-20"=> "padding-top:5rem;padding-bottom:5rem",
        "py-24"=> "padding-top:6rem;padding-bottom:6rem",
        "pt-0" => "padding-top:0",
        "pt-4" => "padding-top:1rem",
        "pt-8" => "padding-top:2rem",
        "pb-4" => "padding-bottom:1rem",
        "pb-8" => "padding-bottom:2rem",
        "pl-4" => "padding-left:1rem",
        "pr-4" => "padding-right:1rem",

        // ── Spacing — Margin ─────────────────────────────────────────────
        "m-0"    => "margin:0",
        "m-auto" => "margin:auto",
        "m-4"    => "margin:1rem",
        "mx-auto"=> "margin-left:auto;margin-right:auto",
        "mx-0"   => "margin-left:0;margin-right:0",
        "mx-4"   => "margin-left:1rem;margin-right:1rem",
        "mx-6"   => "margin-left:1.5rem;margin-right:1.5rem",
        "my-0"   => "margin-top:0;margin-bottom:0",
        "my-4"   => "margin-top:1rem;margin-bottom:1rem",
        "my-8"   => "margin-top:2rem;margin-bottom:2rem",
        "mt-0"   => "margin-top:0",
        "mt-1"   => "margin-top:0.25rem",
        "mt-2"   => "margin-top:0.5rem",
        "mt-3"   => "margin-top:0.75rem",
        "mt-4"   => "margin-top:1rem",
        "mt-6"   => "margin-top:1.5rem",
        "mt-8"   => "margin-top:2rem",
        "mt-10"  => "margin-top:2.5rem",
        "mb-1"   => "margin-bottom:0.25rem",
        "mb-2"   => "margin-bottom:0.5rem",
        "mb-4"   => "margin-bottom:1rem",
        "mb-6"   => "margin-bottom:1.5rem",
        "mb-8"   => "margin-bottom:2rem",
        "mb-12"  => "margin-bottom:3rem",
        "mb-16"  => "margin-bottom:4rem",
        "ml-0"   => "margin-left:0",
        "ml-auto"=> "margin-left:auto",
        "mr-0"   => "margin-right:0",
        "mr-auto"=> "margin-right:auto",

        // ── Gap ──────────────────────────────────────────────────────────
        "gap-0"  => "gap:0",
        "gap-1"  => "gap:0.25rem",
        "gap-2"  => "gap:0.5rem",
        "gap-3"  => "gap:0.75rem",
        "gap-4"  => "gap:1rem",
        "gap-6"  => "gap:1.5rem",
        "gap-8"  => "gap:2rem",
        "gap-10" => "gap:2.5rem",
        "gap-12" => "gap:3rem",

        // ── Typography ───────────────────────────────────────────────────
        "text-xs"    => "font-size:0.75rem;line-height:1rem",
        "text-sm"    => "font-size:0.875rem;line-height:1.25rem",
        "text-base"  => "font-size:1rem;line-height:1.5rem",
        "text-lg"    => "font-size:1.125rem;line-height:1.75rem",
        "text-xl"    => "font-size:1.25rem;line-height:1.75rem",
        "text-2xl"   => "font-size:1.5rem;line-height:2rem",
        "text-3xl"   => "font-size:1.875rem;line-height:2.25rem",
        "text-4xl"   => "font-size:2.25rem;line-height:2.5rem",
        "text-5xl"   => "font-size:3rem;line-height:1",
        "text-6xl"   => "font-size:3.75rem;line-height:1",
        "text-7xl"   => "font-size:4.5rem;line-height:1",
        "text-8xl"   => "font-size:6rem;line-height:1",
        "text-9xl"   => "font-size:8rem;line-height:1",
        "font-thin"       => "font-weight:100",
        "font-light"      => "font-weight:300",
        "font-normal"     => "font-weight:400",
        "font-medium"     => "font-weight:500",
        "font-semibold"   => "font-weight:600",
        "font-bold"       => "font-weight:700",
        "font-extrabold"  => "font-weight:800",
        "font-black"      => "font-weight:900",
        "font-mono"       => "font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace",
        "font-sans"       => "font-family:ui-sans-serif,system-ui,-apple-system,sans-serif",
        "font-serif"      => "font-family:ui-serif,Georgia,Cambria,serif",
        "italic"          => "font-style:italic",
        "not-italic"      => "font-style:normal",
        "tracking-tight"  => "letter-spacing:-0.025em",
        "tracking-normal" => "letter-spacing:0em",
        "tracking-wide"   => "letter-spacing:0.025em",
        "tracking-wider"  => "letter-spacing:0.05em",
        "leading-none"    => "line-height:1",
        "leading-tight"   => "line-height:1.25",
        "leading-normal"  => "line-height:1.5",
        "leading-relaxed" => "line-height:1.625",
        "leading-loose"   => "line-height:2",
        "text-left"       => "text-align:left",
        "text-center"     => "text-align:center",
        "text-right"      => "text-align:right",
        "text-justify"    => "text-align:justify",
        "uppercase"       => "text-transform:uppercase",
        "lowercase"       => "text-transform:lowercase",
        "capitalize"      => "text-transform:capitalize",
        "normal-case"     => "text-transform:none",
        "underline"       => "text-decoration-line:underline",
        "no-underline"    => "text-decoration-line:none",
        "line-through"    => "text-decoration-line:line-through",
        "truncate"        => "overflow:hidden;text-overflow:ellipsis;white-space:nowrap",
        "text-ellipsis"   => "text-overflow:ellipsis",
        "whitespace-nowrap"  => "white-space:nowrap",
        "whitespace-normal"  => "white-space:normal",
        "whitespace-pre"     => "white-space:pre",
        "break-words"        => "overflow-wrap:break-word",
        "break-all"          => "word-break:break-all",

        // ── Colors — Text ────────────────────────────────────────────────
        "text-white"         => "color:#fff",
        "text-black"         => "color:#000",
        "text-transparent"   => "color:transparent",
        "text-slate-50"      => "color:#f8fafc",
        "text-slate-100"     => "color:#f1f5f9",
        "text-slate-200"     => "color:#e2e8f0",
        "text-slate-300"     => "color:#cbd5e1",
        "text-slate-400"     => "color:#94a3b8",
        "text-slate-500"     => "color:#64748b",
        "text-slate-600"     => "color:#475569",
        "text-slate-700"     => "color:#334155",
        "text-slate-800"     => "color:#1e293b",
        "text-slate-900"     => "color:#0f172a",
        "text-slate-950"     => "color:#020617",
        "text-gray-400"      => "color:#9ca3af",
        "text-gray-500"      => "color:#6b7280",
        "text-gray-600"      => "color:#4b5563",
        "text-gray-700"      => "color:#374151",
        "text-gray-900"      => "color:#111827",
        "text-red-400"       => "color:#f87171",
        "text-red-500"       => "color:#ef4444",
        "text-red-600"       => "color:#dc2626",
        "text-blue-100"      => "color:#dbeafe",
        "text-blue-200"      => "color:#bfdbfe",
        "text-blue-300"      => "color:#93c5fd",
        "text-blue-400"      => "color:#60a5fa",
        "text-blue-500"      => "color:#3b82f6",
        "text-blue-600"      => "color:#2563eb",
        "text-green-400"     => "color:#4ade80",
        "text-green-500"     => "color:#22c55e",
        "text-green-600"     => "color:#16a34a",
        "text-yellow-400"    => "color:#facc15",
        "text-yellow-500"    => "color:#eab308",
        "text-purple-400"    => "color:#c084fc",
        "text-purple-500"    => "color:#a855f7",
        "text-pink-400"      => "color:#f472b6",
        "text-pink-500"      => "color:#ec4899",

        // ── Colors — Background ───────────────────────────────────────────
        "bg-white"         => "background-color:#fff",
        "bg-black"         => "background-color:#000",
        "bg-transparent"   => "background-color:transparent",
        "bg-slate-50"      => "background-color:#f8fafc",
        "bg-slate-100"     => "background-color:#f1f5f9",
        "bg-slate-200"     => "background-color:#e2e8f0",
        "bg-slate-700"     => "background-color:#334155",
        "bg-slate-800"     => "background-color:#1e293b",
        "bg-slate-900"     => "background-color:#0f172a",
        "bg-slate-950"     => "background-color:#020617",
        "bg-gray-50"       => "background-color:#f9fafb",
        "bg-gray-100"      => "background-color:#f3f4f6",
        "bg-gray-800"      => "background-color:#1f2937",
        "bg-gray-900"      => "background-color:#111827",
        "bg-red-50"        => "background-color:#fef2f2",
        "bg-red-500"       => "background-color:#ef4444",
        "bg-red-600"       => "background-color:#dc2626",
        "bg-blue-50"       => "background-color:#eff6ff",
        "bg-blue-500"      => "background-color:#3b82f6",
        "bg-blue-600"      => "background-color:#2563eb",
        "bg-blue-900"      => "background-color:#1e3a8a",
        "bg-green-50"      => "background-color:#f0fdf4",
        "bg-green-500"     => "background-color:#22c55e",
        "bg-green-600"     => "background-color:#16a34a",
        "bg-yellow-50"     => "background-color:#fefce8",
        "bg-yellow-400"    => "background-color:#facc15",
        "bg-yellow-600"    => "background-color:#ca8a04",
        "bg-purple-500"    => "background-color:#a855f7",
        "bg-pink-500"      => "background-color:#ec4899",

        // ── Colors — Border ───────────────────────────────────────────────
        "border-transparent" => "border-color:transparent",
        "border-white"       => "border-color:#fff",
        "border-black"       => "border-color:#000",
        "border-slate-100"   => "border-color:#f1f5f9",
        "border-slate-200"   => "border-color:#e2e8f0",
        "border-slate-600"   => "border-color:#475569",
        "border-slate-700"   => "border-color:#334155",
        "border-slate-800"   => "border-color:#1e293b",
        "border-gray-200"    => "border-color:#e5e7eb",
        "border-red-500"     => "border-color:#ef4444",
        "border-blue-500"    => "border-color:#3b82f6",
        "border-green-500"   => "border-color:#22c55e",

        // ── Border ───────────────────────────────────────────────────────
        "border"       => "border-width:1px",
        "border-0"     => "border-width:0",
        "border-2"     => "border-width:2px",
        "border-4"     => "border-width:4px",
        "border-8"     => "border-width:8px",
        "border-t"     => "border-top-width:1px",
        "border-b"     => "border-bottom-width:1px",
        "border-l"     => "border-left-width:1px",
        "border-r"     => "border-right-width:1px",
        "border-solid" => "border-style:solid",
        "border-dashed"=> "border-style:dashed",
        "border-dotted"=> "border-style:dotted",
        "border-none"  => "border-style:none",

        // ── Border Radius ─────────────────────────────────────────────────
        "rounded"     => "border-radius:0.25rem",
        "rounded-sm"  => "border-radius:0.125rem",
        "rounded-md"  => "border-radius:0.375rem",
        "rounded-lg"  => "border-radius:0.5rem",
        "rounded-xl"  => "border-radius:0.75rem",
        "rounded-2xl" => "border-radius:1rem",
        "rounded-3xl" => "border-radius:1.5rem",
        "rounded-full"=> "border-radius:9999px",
        "rounded-none"=> "border-radius:0",
        "rounded-t-xl"=> "border-top-left-radius:0.75rem;border-top-right-radius:0.75rem",
        "rounded-b-xl"=> "border-bottom-left-radius:0.75rem;border-bottom-right-radius:0.75rem",

        // ── Shadow ────────────────────────────────────────────────────────
        "shadow"     => "box-shadow:0 1px 3px 0 rgb(0 0 0/0.1),0 1px 2px -1px rgb(0 0 0/0.1)",
        "shadow-sm"  => "box-shadow:0 1px 2px 0 rgb(0 0 0/0.05)",
        "shadow-md"  => "box-shadow:0 4px 6px -1px rgb(0 0 0/0.1),0 2px 4px -2px rgb(0 0 0/0.1)",
        "shadow-lg"  => "box-shadow:0 10px 15px -3px rgb(0 0 0/0.1),0 4px 6px -4px rgb(0 0 0/0.1)",
        "shadow-xl"  => "box-shadow:0 20px 25px -5px rgb(0 0 0/0.1),0 8px 10px -6px rgb(0 0 0/0.1)",
        "shadow-2xl" => "box-shadow:0 25px 50px -12px rgb(0 0 0/0.25)",
        "shadow-none"=> "box-shadow:none",
        "shadow-inner"=>"box-shadow:inset 0 2px 4px 0 rgb(0 0 0/0.05)",

        // ── Opacity ───────────────────────────────────────────────────────
        "opacity-0"   => "opacity:0",
        "opacity-25"  => "opacity:0.25",
        "opacity-50"  => "opacity:0.5",
        "opacity-75"  => "opacity:0.75",
        "opacity-100" => "opacity:1",

        // ── Overflow ──────────────────────────────────────────────────────
        "overflow-auto"    => "overflow:auto",
        "overflow-hidden"  => "overflow:hidden",
        "overflow-visible" => "overflow:visible",
        "overflow-scroll"  => "overflow:scroll",
        "overflow-x-auto"  => "overflow-x:auto",
        "overflow-y-auto"  => "overflow-y:auto",
        "overflow-x-hidden"=> "overflow-x:hidden",

        // ── Z-index ───────────────────────────────────────────────────────
        "z-0"   => "z-index:0",
        "z-10"  => "z-index:10",
        "z-20"  => "z-index:20",
        "z-30"  => "z-index:30",
        "z-40"  => "z-index:40",
        "z-50"  => "z-index:50",
        "z-auto"=> "z-index:auto",

        // ── Cursor ────────────────────────────────────────────────────────
        "cursor-auto"    => "cursor:auto",
        "cursor-default" => "cursor:default",
        "cursor-pointer" => "cursor:pointer",
        "cursor-wait"    => "cursor:wait",
        "cursor-text"    => "cursor:text",
        "cursor-not-allowed"=> "cursor:not-allowed",

        // ── Pointer Events ────────────────────────────────────────────────
        "pointer-events-none" => "pointer-events:none",
        "pointer-events-auto" => "pointer-events:auto",

        // ── Select ────────────────────────────────────────────────────────
        "select-none" => "user-select:none",
        "select-text" => "user-select:text",
        "select-all"  => "user-select:all",
        "select-auto" => "user-select:auto",

        // ── Transition ────────────────────────────────────────────────────
        "transition"         => "transition-property:color,background-color,border-color,fill,stroke,opacity,box-shadow,transform,filter;transition-timing-function:cubic-bezier(0.4,0,0.2,1);transition-duration:150ms",
        "transition-colors"  => "transition-property:color,background-color,border-color,fill,stroke;transition-timing-function:cubic-bezier(0.4,0,0.2,1);transition-duration:150ms",
        "transition-opacity" => "transition-property:opacity;transition-timing-function:cubic-bezier(0.4,0,0.2,1);transition-duration:150ms",
        "transition-transform"=>"transition-property:transform;transition-timing-function:cubic-bezier(0.4,0,0.2,1);transition-duration:150ms",
        "transition-all"     => "transition-property:all;transition-timing-function:cubic-bezier(0.4,0,0.2,1);transition-duration:150ms",
        "duration-75"        => "transition-duration:75ms",
        "duration-100"       => "transition-duration:100ms",
        "duration-150"       => "transition-duration:150ms",
        "duration-200"       => "transition-duration:200ms",
        "duration-300"       => "transition-duration:300ms",
        "duration-500"       => "transition-duration:500ms",
        "ease-linear"        => "transition-timing-function:linear",
        "ease-in"            => "transition-timing-function:cubic-bezier(0.4,0,1,1)",
        "ease-out"           => "transition-timing-function:cubic-bezier(0,0,0.2,1)",
        "ease-in-out"        => "transition-timing-function:cubic-bezier(0.4,0,0.2,1)",

        // ── Transform ─────────────────────────────────────────────────────
        "scale-95"   => "transform:scale(.95)",
        "scale-100"  => "transform:scale(1)",
        "scale-105"  => "transform:scale(1.05)",
        "scale-110"  => "transform:scale(1.1)",
        "-translate-y-1"  => "transform:translateY(-0.25rem)",
        "-translate-y-2"  => "transform:translateY(-0.5rem)",
        "translate-y-0"   => "transform:translateY(0)",
        "rotate-45"  => "transform:rotate(45deg)",
        "rotate-90"  => "transform:rotate(90deg)",
        "rotate-180" => "transform:rotate(180deg)",

        // ── Misc ──────────────────────────────────────────────────────────
        "sr-only" => "position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border-width:0",
        "not-sr-only" => "position:static;width:auto;height:auto;padding:0;margin:0;overflow:visible;clip:auto;white-space:normal",
        "appearance-none" => "appearance:none",
        "outline-none"    => "outline:2px solid transparent;outline-offset:2px",
        "ring-0"          => "box-shadow:var(--tw-ring-inset) 0 0 0 calc(0px + var(--tw-ring-offset-width)) var(--tw-ring-color)",
        "ring-1"          => "box-shadow:var(--tw-ring-inset) 0 0 0 calc(1px + var(--tw-ring-offset-width)) var(--tw-ring-color)",
        "ring-2"          => "box-shadow:var(--tw-ring-inset) 0 0 0 calc(2px + var(--tw-ring-offset-width)) var(--tw-ring-color)",
        "resize-none"     => "resize:none",
        "resize"          => "resize:both",
        "aspect-square"   => "aspect-ratio:1/1",
        "aspect-video"    => "aspect-ratio:16/9",
        "object-cover"    => "object-fit:cover",
        "object-contain"  => "object-fit:contain",
        "object-center"   => "object-position:center",
        "list-none"       => "list-style-type:none",
        "list-disc"       => "list-style-type:disc",
        "list-decimal"    => "list-style-type:decimal",
        "space-y-1"       => ">*+*{margin-top:0.25rem}",
        "space-y-2"       => ">*+*{margin-top:0.5rem}",
        "space-y-4"       => ">*+*{margin-top:1rem}",
        "space-x-2"       => ">*+*{margin-left:0.5rem}",
        "space-x-4"       => ">*+*{margin-left:1rem}",
        "divide-y"        => ">*+*{border-top-width:1px}",
        "divide-x"        => ">*+*{border-left-width:1px}",

        _ => return None,
    })
}

// ─── Class Extractor ──────────────────────────────────────────────────────────

/// Extract all Tailwind classes from rendered HTML strings.
pub fn extract_classes(html: &str) -> HashSet<String> {
    let mut classes = HashSet::new();
    let mut i = 0;
    let bytes = html.as_bytes();

    while i < bytes.len() {
        // Find class=" or class='
        if i + 7 < bytes.len() && &bytes[i..i+7] == b"class=\"" {
            i += 7;
            let start = i;
            while i < bytes.len() && bytes[i] != b'"' { i += 1; }
            let class_str = &html[start..i];
            for cls in class_str.split_whitespace() {
                classes.insert(cls.to_string());
            }
        } else {
            i += 1;
        }
    }

    classes
}

// ─── CSS Generator ────────────────────────────────────────────────────────────

/// Generate a minimal CSS bundle from a set of used classes.
/// Includes BlissLang animation keyframes + responsive breakpoints.
pub fn generate_css(used_classes: &HashSet<String>) -> String {
    let mut css = String::new();

    // Reset & base
    css.push_str("*,::before,::after{box-sizing:border-box;border-width:0;border-style:solid}\n");
    css.push_str("html{line-height:1.5;-webkit-text-size-adjust:100%}\n");
    css.push_str("body{margin:0;line-height:inherit}\n");
    css.push_str("a{color:inherit;text-decoration:inherit}\n");
    css.push_str("img,video{max-width:100%;height:auto}\n");
    css.push_str("button,input,optgroup,select,textarea{font-family:inherit;font-size:100%;font-weight:inherit;line-height:inherit;color:inherit;margin:0;padding:0}\n");
    css.push_str("button,select{text-transform:none}\n");
    css.push_str("button,[type='button'],[type='reset'],[type='submit']{-webkit-appearance:button;background-color:transparent;background-image:none}\n\n");

    // Forms (BuildForm/Field) — minimal, framework-agnostic base styles.
    // Tailwind utility classes on the wrapping section still apply on top of these.
    css.push_str(".bliss-field{margin-bottom:1rem;display:flex;flex-direction:column;gap:0.375rem}\n");
    css.push_str(".bliss-field label{font-weight:600;font-size:0.9rem}\n");
    css.push_str(".bliss-field input,.bliss-field textarea,.bliss-field select{border:1px solid #cbd5e1;border-radius:0.5rem;padding:0.6rem 0.9rem;font-size:1rem;width:100%}\n");
    css.push_str(".bliss-field input[aria-invalid='true'],.bliss-field textarea[aria-invalid='true'],.bliss-field select[aria-invalid='true']{border-color:#ef4444}\n");
    css.push_str(".bliss-field-error{color:#ef4444;font-size:0.85rem;min-height:1rem;display:block}\n");
    css.push_str(".bliss-field-hint{color:#64748b;font-size:0.8rem;margin:0.25rem 0 0}\n");
    css.push_str(".bliss-radio-option{display:flex;align-items:center;gap:0.5rem;font-weight:400}\n");
    css.push_str(".bliss-form-submit{cursor:pointer}\n");
    css.push_str(".bliss-form-submit:disabled{opacity:0.6;cursor:not-allowed}\n");
    css.push_str(".bliss-form-status{font-size:0.9rem;margin-top:0.5rem}\n");
    css.push_str(".bliss-form-status.bliss-form-success{color:#16a34a}\n");
    css.push_str(".bliss-form-status.bliss-form-error{color:#ef4444}\n\n");

    // Utility classes
    for class in used_classes {
        // Handle responsive prefixes: md:grid-cols-3, lg:flex, etc.
        let (prefix, base) = if let Some(idx) = class.find(':') {
            let p = &class[..idx];
            let b = &class[idx+1..];
            (Some(p), b)
        } else {
            (None, class.as_str())
        };

        let resolved = lookup(base).map(str::to_string).or_else(|| arbitrary_rule(base));

        if let Some(rule) = resolved {
            let selector = format!(".{}", escape_class(class));
            match prefix {
                None => {
                    css.push_str(&format!("{}{{{};}}\n", selector, rule));
                }
                Some("md") => {
                    css.push_str(&format!("@media(min-width:768px){{{}{{{};}}}}\n", selector, rule));
                }
                Some("lg") => {
                    css.push_str(&format!("@media(min-width:1024px){{{}{{{};}}}}\n", selector, rule));
                }
                Some("sm") => {
                    css.push_str(&format!("@media(min-width:640px){{{}{{{};}}}}\n", selector, rule));
                }
                Some("xl") => {
                    css.push_str(&format!("@media(min-width:1280px){{{}{{{};}}}}\n", selector, rule));
                }
                Some("2xl") => {
                    css.push_str(&format!("@media(min-width:1536px){{{}{{{};}}}}\n", selector, rule));
                }
                Some("hover") => {
                    css.push_str(&format!("{}:hover{{{};}}\n", selector, rule));
                }
                Some("focus") => {
                    css.push_str(&format!("{}:focus{{{};}}\n", selector, rule));
                }
                Some("active") => {
                    css.push_str(&format!("{}:active{{{};}}\n", selector, rule));
                }
                Some("dark") => {
                    css.push_str(&format!("@media(prefers-color-scheme:dark){{{}{{{};}}}}\n", selector, rule));
                }
                _ => {} // unknown prefix — skip
            }
        }
    }

    // BlissLang animation keyframes
    css.push_str("\n/* BlissLang Animations */\n");
    css.push_str("[data-animate]{opacity:0}\n");
    css.push_str("[data-animate].bliss-visible{animation-fill-mode:both}\n");
    css.push_str("@keyframes bliss-fadeIn{from{opacity:0}to{opacity:1}}\n");
    css.push_str("@keyframes bliss-fadeInUp{from{opacity:0;transform:translateY(20px)}to{opacity:1;transform:translateY(0)}}\n");
    css.push_str("@keyframes bliss-fadeInDown{from{opacity:0;transform:translateY(-20px)}to{opacity:1;transform:translateY(0)}}\n");
    css.push_str("@keyframes bliss-fadeInLeft{from{opacity:0;transform:translateX(-20px)}to{opacity:1;transform:translateX(0)}}\n");
    css.push_str("@keyframes bliss-fadeInRight{from{opacity:0;transform:translateX(20px)}to{opacity:1;transform:translateX(0)}}\n");
    css.push_str("@keyframes bliss-zoomIn{from{opacity:0;transform:scale(.9)}to{opacity:1;transform:scale(1)}}\n");
    css.push_str("@keyframes bliss-slideInLeft{from{transform:translateX(-100%)}to{transform:translateX(0)}}\n");
    css.push_str("@keyframes bliss-slideInRight{from{transform:translateX(100%)}to{transform:translateX(0)}}\n");
    css.push_str("@keyframes bliss-slideInUp{from{transform:translateY(100%)}to{transform:translateY(0)}}\n");
    css.push_str("@keyframes bliss-bounceIn{0%{transform:scale(.3);opacity:0}50%{transform:scale(1.05)}70%{transform:scale(.9)}100%{transform:scale(1);opacity:1}}\n");
    css.push_str("@keyframes bliss-pulse{0%,100%{transform:scale(1)}50%{transform:scale(1.05)}}\n");
    css.push_str("@keyframes bliss-shake{0%,100%{transform:translateX(0)}20%{transform:translateX(-8px)}40%{transform:translateX(8px)}60%{transform:translateX(-8px)}80%{transform:translateX(8px)}}\n");
    css.push_str("@keyframes bliss-bounce{0%,100%{transform:translateY(0)}50%{transform:translateY(-10px)}}\n");
    css.push_str("@keyframes bliss-spin{from{transform:rotate(0deg)}to{transform:rotate(360deg)}}\n");
    css.push_str("@keyframes bliss-flipInX{from{transform:perspective(400px) rotateX(90deg);opacity:0}to{transform:perspective(400px) rotateX(0deg);opacity:1}}\n");

    // Responsive visibility helpers
    css.push_str(".bliss-mobile-only{display:block}\n");
    css.push_str(".bliss-tablet-only{display:none}\n");
    css.push_str(".bliss-desktop-only{display:none}\n");
    css.push_str("@media(min-width:768px){.bliss-mobile-only{display:none}.bliss-tablet-only{display:block}}\n");
    css.push_str("@media(min-width:1024px){.bliss-tablet-only{display:none}.bliss-desktop-only{display:block}}\n");

    css
}

/// Escape a CSS class name so it's a valid selector.
/// e.g. "md:grid-cols-3" → "md\\:grid-cols-3"
/// Resolve a Tailwind **arbitrary-value** class — `bg-[url('/img.png')]`,
/// `w-[320px]`, `text-[#1a1a2e]`, `[mask-type:luminance]`, etc. — into a raw
/// CSS declaration body. Returns `None` if `class` isn't in `prefix-[value]`
/// (or bare `[property:value]`) form, or the prefix isn't recognised.
///
/// Follows the Tailwind convention that underscores inside the brackets
/// stand in for literal spaces (since class names can't contain spaces).
pub(crate) fn arbitrary_rule(class: &str) -> Option<String> {
    if !class.ends_with(']') {
        return None;
    }
    let lb = class.find('[')?;
    let rb = class.rfind(']')?;
    if rb <= lb {
        return None;
    }

    let prefix = class[..lb].strip_suffix('-').unwrap_or(&class[..lb]);
    let raw_value = &class[lb + 1..rb];
    if raw_value.is_empty() {
        return None;
    }
    let value = raw_value.replace('_', " ");

    // Bare arbitrary property: `[mask-type:luminance]`, `[grid-column:span_2]`
    if prefix.is_empty() {
        let idx = raw_value.find(':')?;
        let prop = &raw_value[..idx];
        let val = raw_value[idx + 1..].replace('_', " ");
        if prop.is_empty() || val.is_empty() {
            return None;
        }
        return Some(format!("{}:{}", prop, val));
    }

    let looks_like_length = |v: &str| {
        let v = v.trim();
        v.ends_with("px") || v.ends_with("rem") || v.ends_with("em") || v.ends_with('%')
            || v.ends_with("vh") || v.ends_with("vw") || v.ends_with("ch") || v.ends_with("deg")
            || v.parse::<f64>().is_ok()
    };

    Some(match prefix {
        "bg" => {
            if value.starts_with("url(") {
                format!("background-image:{}", value)
            } else {
                format!("background-color:{}", value)
            }
        }
        "text"   => if looks_like_length(&value) { format!("font-size:{}", value) } else { format!("color:{}", value) },
        "border" => if looks_like_length(&value) { format!("border-width:{}", value) } else { format!("border-color:{}", value) },
        "w"        => format!("width:{}", value),
        "h"        => format!("height:{}", value),
        "min-w"    => format!("min-width:{}", value),
        "min-h"    => format!("min-height:{}", value),
        "max-w"    => format!("max-width:{}", value),
        "max-h"    => format!("max-height:{}", value),
        "top"      => format!("top:{}", value),
        "left"     => format!("left:{}", value),
        "right"    => format!("right:{}", value),
        "bottom"   => format!("bottom:{}", value),
        "inset"    => format!("inset:{}", value),
        "p"        => format!("padding:{}", value),
        "px"       => format!("padding-left:{};padding-right:{}", value, value),
        "py"       => format!("padding-top:{};padding-bottom:{}", value, value),
        "pt"       => format!("padding-top:{}", value),
        "pb"       => format!("padding-bottom:{}", value),
        "pl"       => format!("padding-left:{}", value),
        "pr"       => format!("padding-right:{}", value),
        "m"        => format!("margin:{}", value),
        "mx"       => format!("margin-left:{};margin-right:{}", value, value),
        "my"       => format!("margin-top:{};margin-bottom:{}", value, value),
        "mt"       => format!("margin-top:{}", value),
        "mb"       => format!("margin-bottom:{}", value),
        "ml"       => format!("margin-left:{}", value),
        "mr"       => format!("margin-right:{}", value),
        "gap"      => format!("gap:{}", value),
        "rounded"  => format!("border-radius:{}", value),
        "z"        => format!("z-index:{}", value),
        "opacity"  => format!("opacity:{}", value),
        "leading"  => format!("line-height:{}", value),
        "tracking" => format!("letter-spacing:{}", value),
        "translate-x" => format!("transform:translateX({})", value),
        "translate-y" => format!("transform:translateY({})", value),
        "rotate"   => format!("transform:rotate({})", value),
        "scale"    => format!("transform:scale({})", value),
        "fill"     => format!("fill:{}", value),
        "stroke"   => format!("stroke:{}", value),
        _ => return None,
    })
}

fn escape_class(class: &str) -> String {
    class
        .replace(':', "\\:")
        .replace('/', "\\/")
        .replace('.', "\\.")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('!', "\\!")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('\'', "\\'")
        .replace('#', "\\#")
        .replace('%', "\\%")
        .replace(',', "\\,")
}

/// Build the complete purged CSS from a list of HTML pages.
pub fn build_purged_css(html_pages: &[&str]) -> String {
    let mut all_classes = HashSet::new();
    for html in html_pages {
        all_classes.extend(extract_classes(html));
    }
    generate_css(&all_classes)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_known_class() {
        assert_eq!(lookup("flex"),      Some("display:flex"));
        assert_eq!(lookup("text-white"),Some("color:#fff"));
        assert_eq!(lookup("p-4"),       Some("padding:1rem"));
        assert_eq!(lookup("rounded-lg"),Some("border-radius:0.5rem"));
    }

    #[test]
    fn test_lookup_unknown_returns_none() {
        assert!(lookup("not-a-tailwind-class").is_none());
        assert!(lookup("bg-bloo-500").is_none());
    }

    #[test]
    fn test_arbitrary_rule_background_image() {
        let rule = arbitrary_rule("bg-[url('/Assets/Image/bg.png')]").unwrap();
        assert_eq!(rule, "background-image:url('/Assets/Image/bg.png')");
    }

    #[test]
    fn test_arbitrary_rule_background_color() {
        assert_eq!(arbitrary_rule("bg-[#1a1a2e]").unwrap(), "background-color:#1a1a2e");
    }

    #[test]
    fn test_arbitrary_rule_length_vs_color_for_text() {
        assert_eq!(arbitrary_rule("text-[2rem]").unwrap(), "font-size:2rem");
        assert_eq!(arbitrary_rule("text-[#ff0000]").unwrap(), "color:#ff0000");
    }

    #[test]
    fn test_arbitrary_rule_underscore_becomes_space() {
        assert_eq!(arbitrary_rule("bg-[center_top]").unwrap(), "background-color:center top");
    }

    #[test]
    fn test_arbitrary_rule_bare_property() {
        assert_eq!(arbitrary_rule("[mask-type:luminance]").unwrap(), "mask-type:luminance");
    }

    #[test]
    fn test_arbitrary_rule_unknown_prefix_returns_none() {
        assert!(arbitrary_rule("frobnicate-[123]").is_none());
    }

    #[test]
    fn test_arbitrary_rule_not_bracket_form_returns_none() {
        assert!(arbitrary_rule("bg-cover").is_none());
    }

    #[test]
    fn test_generate_css_resolves_arbitrary_background_image() {
        let mut used = HashSet::new();
        used.insert("bg-[url('/Assets/Image/bg.png')]".to_string());
        let css = generate_css(&used);
        assert!(css.contains("background-image:url('/Assets/Image/bg.png')"));
    }

    #[test]
    fn test_generate_css_arbitrary_with_responsive_prefix() {
        let mut used = HashSet::new();
        used.insert("md:w-[320px]".to_string());
        let css = generate_css(&used);
        assert!(css.contains("@media(min-width:768px)"));
        assert!(css.contains("width:320px"));
    }

    #[test]
    fn test_extract_classes() {
        let html = r#"<div class="flex items-center gap-4"><p class="text-white font-bold">Hello</p></div>"#;
        let classes = extract_classes(html);
        assert!(classes.contains("flex"));
        assert!(classes.contains("items-center"));
        assert!(classes.contains("gap-4"));
        assert!(classes.contains("text-white"));
        assert!(classes.contains("font-bold"));
    }

    #[test]
    fn test_generate_css_contains_used_classes() {
        let mut used = HashSet::new();
        used.insert("flex".to_string());
        used.insert("text-white".to_string());
        used.insert("p-4".to_string());
        let css = generate_css(&used);
        assert!(css.contains(".flex"));
        assert!(css.contains(".text-white"));
        assert!(css.contains(".p-4"));
        assert!(css.contains("bliss-fadeIn")); // animations always included
    }

    #[test]
    fn test_responsive_prefix() {
        let mut used = HashSet::new();
        used.insert("md:grid-cols-3".to_string());
        let css = generate_css(&used);
        assert!(css.contains("min-width:768px"), "Missing md breakpoint in: {}", &css[..200.min(css.len())]);
    }

    #[test]
    fn test_escape_class() {
        assert_eq!(escape_class("md:grid-cols-3"), "md\\:grid-cols-3");
        assert_eq!(escape_class("hover:bg-red-500"), "hover\\:bg-red-500");
    }

    #[test]
    fn test_build_purged_css_end_to_end() {
        let html = r#"<section class="bg-slate-900 min-h-screen flex flex-col items-center"><h1 class="text-6xl font-bold text-white">Hello</h1></section>"#;
        let css = build_purged_css(&[html]);
        assert!(css.contains(".bg-slate-900"));
        assert!(css.contains(".min-h-screen"));
        assert!(css.contains(".flex"));
        assert!(css.contains(".text-white"));
        assert!(css.contains(".font-bold"));
    }
}
