## ADDED Requirements

### Requirement: EChart wrapper component manages ECharts lifecycle in SolidJS
`frontend/src/components/ui/EChart.tsx` SHALL export a SolidJS component with the following props:

```typescript
interface EChartProps {
  option: () => EChartsOption;  // accessor for reactive option
  class?: string;
  height?: string;              // default "320px"
}
```

Lifecycle management:
- `onMount`: create ECharts instance via `echarts.init(containerEl)`, call `setOption(props.option())`
- `createEffect`: when `props.option()` changes, call `instance.setOption(newOption, { notMerge: true })`
- ResizeObserver: attach to container element, call `instance.resize()` on size change (handles sidebar collapse)
- `onCleanup`: dispose ECharts instance, disconnect ResizeObserver

ECharts import strategy — tree-shaking via `echarts/core`:
- `echarts/core`: `use([CanvasRenderer])`
- `echarts/charts`: `LineChart`, `BarChart`
- `echarts/components`: `GridComponent`, `TooltipComponent`, `LegendComponent`

Theme integration:
- On mount and on `.dark` class change on `document.documentElement`, read CSS variables via `getComputedStyle()`:
  - `--content` → axis label color, legend text color
  - `--content-tertiary` → axis line color, split line color
  - `--border` → grid border color
  - `--surface-elevated` → tooltip background
  - `--accent`, `--success`, `--info`, `--warning`, `--error` → series palette
- Use MutationObserver on `document.documentElement` attributes to detect class changes for theme switch; on change, rebuild option colors and call `setOption` with updated theme colors

Container element: `<div>` with `style={{ height: props.height ?? '320px' }}` and `class={props.class}`.

#### Scenario: EChart mounts and renders
- **WHEN** `<EChart option={chartOption} />` is mounted
- **THEN** an ECharts canvas instance is created inside the container div with the provided option

#### Scenario: Option reactively updates
- **WHEN** the `option` accessor returns a new value
- **THEN** `setOption` is called with `notMerge: true`, replacing the previous chart configuration

#### Scenario: Container resizes due to sidebar collapse
- **WHEN** the admin sidebar toggles between collapsed (w-16) and expanded (w-56)
- **THEN** ResizeObserver fires and the ECharts instance calls `.resize()`, re-fitting the chart

#### Scenario: Theme switches from light to dark
- **WHEN** `document.documentElement.classList` adds/removes `dark`
- **THEN** EChart re-reads CSS variables and updates axis/tooltip/grid colors via `setOption`

#### Scenario: Component unmounts
- **WHEN** the component is removed from the DOM
- **THEN** ECharts instance is disposed, ResizeObserver and MutationObserver are disconnected; no memory leaks
