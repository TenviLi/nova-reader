<script lang="ts">
	let { progress = 0, size = 'md', showLabel = true } = $props<{
		progress: number;
		size?: 'sm' | 'md' | 'lg';
		showLabel?: boolean;
	}>();

	const percent = $derived(Math.round(progress * 100));
	const circumference = 2 * Math.PI * 18;
	const dashOffset = $derived(circumference - (progress * circumference));

	const sizes: Record<string, string> = {
		sm: 'w-8 h-8',
		md: 'w-12 h-12',
		lg: 'w-16 h-16',
	};

	const progressColor = $derived(
		percent >= 100 ? 'stroke-emerald-500' :
		percent >= 60 ? 'stroke-accent-500' :
		percent >= 30 ? 'stroke-amber-500' :
		'stroke-ink-400'
	);
	const progressLabel = $derived(
		percent >= 100 ? '已读完' :
		percent >= 60 ? '阅读中' :
		percent >= 30 ? '阅读中' :
		'未开始'
	);
</script>

<div class="inline-flex flex-col items-center gap-1" role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100} aria-label="阅读进度: {percent}% ({progressLabel})">
	<div class="relative {sizes[size]}">
		<svg class="w-full h-full -rotate-90" viewBox="0 0 40 40">
			<!-- Background circle -->
			<circle
				cx="20" cy="20" r="18"
				class="fill-none stroke-ink-800/30"
				stroke-width="3"
			/>
			<!-- Progress circle -->
			<circle
				cx="20" cy="20" r="18"
				class="fill-none {progressColor} transition-all duration-500"
				stroke-width="3"
				stroke-linecap="round"
				stroke-dasharray={circumference}
				stroke-dashoffset={dashOffset}
			/>
		</svg>
		{#if size !== 'sm'}
			<span class="absolute inset-0 flex items-center justify-center text-xs font-medium text-ink-300">
				{percent}%
			</span>
		{/if}
	</div>
	{#if showLabel && size === 'lg'}
		<span class="text-xs text-ink-500">
			{#if percent >= 100}
				已读完
			{:else if percent > 0}
				阅读中
			{:else}
				未开始
			{/if}
		</span>
	{/if}
</div>
