/* WordForge admin redesign — shared UI layer (handoff port). */
export { Icon, IconPaths } from './Icon';
export { sx, cx } from './sx';
export {
  Sparkline, Legend, LineChart, BarChart, Donut, Heatmap, Scatter,
  type Series, type BarDatum, type DonutDatum, type ScatterPoint,
} from './charts';
export {
  Btn, IconBtn, Badge, Card, Field, Switch, Tabs, Seg, Empty, Skel, Spinner, Loading,
  Modal, Drawer, Confirm, StatCard, Panel, Progress, Pagination, PageHead,
} from './primitives';
export { fmtNum, fmtBytes, fmtDur, fmtTime, fmtAgo, fmtPct } from './format';

import { uiStore } from '@/stores/ui';
/** toast — matches the handoff `useToast()` surface, backed by the existing uiStore. */
export const toast = uiStore.toast;
