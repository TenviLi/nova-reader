<script lang="ts">
	import { api } from '$services/api';
	import { Radar } from 'lucide-svelte';

	let { bookId } = $props<{ bookId: string }>();

	interface RadarAxis { name: string; color: string; score: number }

	let axes = $state<RadarAxis[]>([]);
	let loading = $state(true);

	$effect(() => {
		loadRadar();
	});

	async function loadRadar() {
		loading = true;
		try {
			const result = await api.getBookRadar(bookId);
			axes = result?.axes ?? [];
		} catch { axes = []; }
		finally { loading = false; }
	}

	function radarPath(): string {
		if (axes.length < 3) return '';
		const cx = 150, cy = 150, r = 110;
		const n = axes.length;
		const points = axes.map((a, i) => {
			const angle = (Math.PI * 2 * i) / n - Math.PI / 2;
			const val = Math.max(0, Math.min(1, a.score));
			return `${cx + r * val * Math.cos(angle)},${cy + r * val * Math.sin(angle)}`;
		});
		return `M ${points.join(' L ')} Z`;
	}

	function gridPath(level: number): string {
		const cx = 150, cy = 150, r = 110;
		const n = axes.length;
		const points = Array.from({ length: n }, (_, i) => {
			const angle = (Math.PI * 2 * i) / n - Math.PI / 2;
			return `${cx + r * level * Math.cos(angle)},${cy + r * level * Math.sin(angle)}`;
		});
		return `M ${points.join(' L ')} Z`;
	}
</script>

{#if loading}
	<div class="text-center py-8 text-ink-500">加载中...</div>
{:else if axes.length < 3}
	<div class="text-center py-8 text-ink-500">
		<Radar size={32} class="mx-auto mb-2 opacity-30" />
		<p class="text-sm">标签数据不足以生成雷达图</p>
		<p class="text-xs mt-1 text-ink-600">需要至少 3 个标签画像的分数</p>
	</div>
{:else}
	<div class="flex flex-col items-center gap-4">
		<svg viewBox="0 0 300 300" class="w-64 h-64">
			<!-- Grid levels -->
			{#each [0.25, 0.5, 0.75, 1.0] as level}
				<path d={gridPath(level)} fill="none" stroke="currentColor" class="text-ink-800" stroke-width="0.5" />
			{/each}
			<!-- Axis lines -->
			{#each axes as _, i}
				{@const angle = (Math.PI * 2 * i) / axes.length - Math.PI / 2}
				<line x1="150" y1="150" x2={150 + 110 * Math.cos(angle)} y2={150 + 110 * Math.sin(angle)} stroke="currentColor" class="text-ink-800" stroke-width="0.5" />
			{/each}
			<!-- Data polygon -->
			<path d={radarPath()} fill="rgba(245, 158, 11, 0.12)" stroke="#f59e0b" stroke-width="2" stroke-linejoin="round" />
			<!-- Data points -->
			{#each axes as axis, i}
				{@const angle = (Math.PI * 2 * i) / axes.length - Math.PI / 2}
				{@const val = Math.max(0, Math.min(1, axis.score))}
				<circle
					cx={150 + 110 * val * Math.cos(angle)}
					cy={150 + 110 * val * Math.sin(angle)}
					r="3"
					fill={axis.color}
					stroke="white"
					stroke-width="0.5"
				/>
			{/each}
			<!-- Labels -->
			{#each axes as axis, i}
				{@const angle = (Math.PI * 2 * i) / axes.length - Math.PI / 2}
				{@const lx = 150 + 132 * Math.cos(angle)}
				{@const ly = 150 + 132 * Math.sin(angle)}
				<text x={lx} y={ly} text-anchor="middle" dominant-baseline="middle" class="fill-ink-400 text-[9px]">
					{axis.name}
				</text>
			{/each}
		</svg>

		<!-- Legend below chart -->
		<div class="flex flex-wrap gap-3 justify-center">
			{#each axes as axis}
				<div class="flex items-center gap-1.5">
					<div class="w-2.5 h-2.5 rounded-full" style="background-color: {axis.color}"></div>
					<span class="text-xs text-ink-400">{axis.name}</span>
					<span class="text-xs text-ink-600">{(axis.score * 100).toFixed(0)}%</span>
				</div>
			{/each}
		</div>
	</div>
{/if}
