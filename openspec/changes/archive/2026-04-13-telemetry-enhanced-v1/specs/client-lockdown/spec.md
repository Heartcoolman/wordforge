## ADDED Requirements

### Requirement: Client locks itself on receipt of data_corrupted SSE event
The frontend SHALL handle the `data_corrupted` SSE event by entering an irrecoverable locked state. This state persists for the lifetime of the page/app session and CANNOT be dismissed by the user.

#### New component: `frontend/src/components/SystemLockedModal.tsx`
A full-screen overlay component rendered above all other content (z-index highest). Properties:
- Fixed position, covers entire viewport
- Semi-transparent dark backdrop (`bg-black/70`)
- Centered modal card (non-dismissable: no close button, clicking backdrop does nothing)
- Content:
  - Icon: warning/error visual
  - Title: "数据损坏"
  - Body: "客户端数据已损坏，请重启应用后再试。"
  - No action buttons (cannot be dismissed)
- Pointer events on backdrop are disabled to prevent any click-through

#### App.tsx integration
`frontend/src/App.tsx` gains a new reactive signal:
```typescript
const [systemLocked, setSystemLocked] = createSignal(false);
```

The existing SSE event handler (in the realtime/SSE setup code) gains a new case:
```typescript
case 'data_corrupted':
  setSystemLocked(true);
  break;
```

The root JSX renders the modal via Solid Portal to guarantee it sits above all existing z-index layers:
```tsx
<Show when={systemLocked()}>
  <Portal mount={document.body}>
    <SystemLockedModal />
  </Portal>
</Show>
```

#### Behavior when locked
- The modal overlay (rendered via Solid `<Portal mount={document.body}>`) prevents all pointer interaction with underlying content; its z-index MUST exceed all existing layers (drawers, Toasts, maintenance page)
- The browser tab/window can still be closed (this is expected; user is prompted to "restart")
- Keyboard shortcuts that do not require UI interaction (e.g., Ctrl+W to close tab) still work
- **SSE connection and telemetry worker continue running uninterrupted** — the client keeps sending heartbeats; if the server's miss counter resets, no automatic UI unlock occurs
- The locked state is in-memory only; a page refresh or app restart clears it (this is the intended recovery path)

#### Scenario: SSE delivers data_corrupted — modal appears
- **WHEN** the client receives `{ "type": "data_corrupted" }` via SSE
- **THEN** `systemLocked` signal becomes `true`
- **THEN** `SystemLockedModal` renders over the entire UI immediately
- **THEN** all buttons, inputs, and navigation are inaccessible behind the overlay

#### Scenario: Modal cannot be dismissed
- **WHEN** user clicks anywhere on the locked screen
- **THEN** nothing happens; the modal remains
- **WHEN** user presses Escape
- **THEN** nothing happens; the modal remains

#### Scenario: Page refresh clears lock (expected behavior)
- **WHEN** user refreshes the page (F5 / Ctrl+R)
- **THEN** `systemLocked` resets to `false` (signal is in-memory, not persisted)
- **THEN** SSE reconnects; if the server still considers the device missing heartbeats, it will fire `data_corrupted` again within 25 seconds
- **NOTE**: This is intentional — "restart the client" means refreshing or relaunching, which re-establishes normal heartbeat flow

#### Scenario: SseEvent enum extension (Rust)
- `src/state.rs` MUST add:
  ```rust
  #[serde(rename = "data_corrupted")]
  DataCorrupted,
  ```
- `src/routes/admin/clients.rs` watchdog or `heartbeat_watchdog.rs` sends `SseEvent::DataCorrupted` to all connections of the target device
