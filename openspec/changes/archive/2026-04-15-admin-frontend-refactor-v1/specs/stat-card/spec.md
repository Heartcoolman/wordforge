## ADDED Requirements

### Requirement: StatCard component renders KPI with icon, color, and optional trend
`frontend/src/components/ui/StatCard.tsx` SHALL export a SolidJS component with the following props:

```typescript
interface StatCardProps {
  title: string;
  value: string | number;
  icon: string;          // SVG path d attribute (same format as AdminLayout sidebarLinks)
  color: 'accent' | 'success' | 'warning' | 'error' | 'info';
  trend?: { value: number; label: string };
}
```

Rendering rules:
- Container uses `Card` component with `variant="elevated"` and `padding="lg"`
- Icon rendered as 24×24 SVG with `stroke="currentColor"` in a 40×40 rounded-lg background using the semantic color's light variant (e.g. `bg-accent-light text-accent`)
- `value` rendered as `text-3xl font-bold` in the semantic color (e.g. `text-accent`)
- `title` rendered as `text-sm text-content-secondary`
- When `trend` is provided: render below value as `text-xs`; positive `trend.value` shows `↑` prefix with `text-success`, negative shows `↓` with `text-error`, zero shows `→` with `text-content-tertiary`; `trend.label` appended after the value (e.g. "↑ 12% 较昨日")
- When `trend` is omitted: no trend area rendered

#### Scenario: StatCard with all props
- **WHEN** `<StatCard title="注册用户" value={1234} icon="M12..." color="accent" trend={{ value: 12, label: "较昨日" }} />` is rendered
- **THEN** the card shows: accent-colored icon background, "1234" in accent color, "注册用户" label, and "↑ 12% 较昨日" in success color

#### Scenario: StatCard without trend
- **WHEN** `<StatCard title="总单词" value="5,678" icon="M12..." color="info" />` is rendered
- **THEN** the card shows icon, value, and title only; no trend indicator is present

#### Scenario: StatCard with negative trend
- **WHEN** `trend={{ value: -5, label: "较昨日" }}` is provided
- **THEN** trend displays "↓ 5% 较昨日" in error color

#### Scenario: StatCard with zero trend
- **WHEN** `trend={{ value: 0, label: "较昨日" }}` is provided
- **THEN** trend displays "→ 0% 较昨日" in content-tertiary color
