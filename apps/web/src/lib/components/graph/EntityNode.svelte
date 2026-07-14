<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';

	let { data } = $props<{
		data: {
			label: string;
			entityType: string;
			size: number;
			color: string;
		};
	}>();

	const typeIcons: Record<string, string> = {
		person: '👤',
		location: '📍',
		organization: '🏛️',
		item: '⚔️',
		concept: '💡',
		event: '📌',
	};
</script>

<div
	class="flex items-center gap-1.5 rounded-full border-2 px-3 py-1.5 shadow-lg backdrop-blur-sm transition-transform hover:scale-110"
	style="border-color: {data.color}; background: {data.color}15; min-width: {Math.max(60, data.size * 8)}px"
>
	<Handle type="target" position={Position.Left} style="opacity: 0; width: 8px; height: 8px;" />

	<span class="text-xs">{typeIcons[data.entityType] ?? '•'}</span>
	<span class="text-xs font-medium text-ink-100 truncate max-w-24" title={data.label}>
		{data.label}
	</span>

	<Handle type="source" position={Position.Right} style="opacity: 0; width: 8px; height: 8px;" />
</div>
