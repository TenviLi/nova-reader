<script lang="ts">
	let { class: className = '', variant = 'default', ...rest } = $props<{
		class?: string;
		variant?: 'default' | 'circle' | 'text' | 'card';
	}>();

	const variantClasses: Record<string, string> = {
		default: 'rounded-lg',
		circle: 'rounded-full',
		text: 'rounded h-4 w-3/4',
		card: 'rounded-xl h-32',
	};
</script>

<div
	class="skeleton-shimmer bg-ink-800/30 {variantClasses[variant]} {className}"
	{...rest}
></div>

<style>
	.skeleton-shimmer {
		position: relative;
		overflow: hidden;
	}
	.skeleton-shimmer::after {
		content: '';
		position: absolute;
		inset: 0;
		background: linear-gradient(
			90deg,
			transparent 0%,
			rgba(255, 255, 255, 0.04) 50%,
			transparent 100%
		);
		animation: shimmer 1.5s ease-in-out infinite;
	}
	@keyframes shimmer {
		0% { transform: translateX(-100%); }
		100% { transform: translateX(100%); }
	}
</style>
