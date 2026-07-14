<script lang="ts">
	import { toastStore } from '$stores/toast.svelte';
	import { Check, X, AlertTriangle, Info } from 'lucide-svelte';

	const typeStyles: Record<string, { bg: string; icon: string; border: string }> = {
		success: { bg: 'bg-emerald-50', icon: 'text-emerald-500', border: 'border-emerald-200' },
		error: { bg: 'bg-red-50', icon: 'text-red-500', border: 'border-red-200' },
		info: { bg: 'bg-blue-50', icon: 'text-blue-500', border: 'border-blue-200' },
		warning: { bg: 'bg-amber-50', icon: 'text-amber-500', border: 'border-amber-200' },
	};
</script>

{#if toastStore.toasts.length > 0}
	<div class="fixed bottom-4 right-4 z-50 flex flex-col gap-2 max-w-sm">
		{#each toastStore.toasts as toast (toast.id)}
			{@const style = typeStyles[toast.type]}
			<div
				class="flex items-start gap-3 p-4 rounded-xl border shadow-lg backdrop-blur-sm {style.bg} {style.border} animate-slide-up"
				role="alert"
			>
				<!-- Icon -->
				<div class="shrink-0 mt-0.5 {style.icon}">
					{#if toast.type === 'success'}
						<Check size={18} strokeWidth={2.5} />
					{:else if toast.type === 'error'}
						<X size={18} strokeWidth={2.5} />
					{:else if toast.type === 'warning'}
						<AlertTriangle size={18} strokeWidth={2} />
					{:else}
						<Info size={18} strokeWidth={2} />
					{/if}
				</div>

				<!-- Content -->
				<div class="flex-1 min-w-0">
					<p class="text-sm font-medium text-ink-800">{toast.title}</p>
					{#if toast.message}
						<p class="text-xs text-ink-500 mt-0.5">{toast.message}</p>
					{/if}
				</div>

				<!-- Dismiss -->
				<button
					onclick={() => toastStore.dismiss(toast.id)}
					class="shrink-0 p-0.5 text-ink-400 hover:text-ink-600 rounded"
				>
					<X size={16} strokeWidth={2} />
				</button>
			</div>
		{/each}
	</div>
{/if}
