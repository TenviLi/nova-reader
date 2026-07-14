<script lang="ts">
	let { range, onclose, onsave } = $props<{
		range: { start: number; end: number };
		onclose: () => void;
		onsave: (note: string, color: string) => void;
	}>();

	let note = $state('');
	let selectedColor = $state('#f59e0b');
	const noteId = $props.id();

	const colors = [
		{ value: '#f59e0b', label: '黄色' },
		{ value: '#10b981', label: '绿色' },
		{ value: '#6366f1', label: '蓝色' },
		{ value: '#ec4899', label: '粉色' },
		{ value: '#ef4444', label: '红色' },
	];
</script>

<div class="fixed inset-0 z-50">
	<button
		type="button"
		class="absolute inset-0 h-full w-full cursor-default bg-transparent"
		aria-label="关闭批注弹窗"
		onclick={onclose}
	></button>

	<div
		role="dialog"
		aria-modal="true"
		aria-label="添加批注"
		tabindex="-1"
		class="absolute left-1/2 top-1/2 z-10 w-80 -translate-x-1/2 -translate-y-1/2 rounded-xl border border-ink-700/50 bg-ink-900 p-4 shadow-2xl"
	>
		<h4 class="mb-3 text-sm font-medium text-ink-100">添加批注</h4>

		<div class="mb-3 flex gap-2" role="group" aria-label="批注颜色">
			{#each colors as color}
				<button
					type="button"
					onclick={() => selectedColor = color.value}
					class="h-6 w-6 rounded-full border-2 transition-transform focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300 focus-visible:ring-offset-2 focus-visible:ring-offset-ink-900"
					class:scale-125={selectedColor === color.value}
					class:border-white={selectedColor === color.value}
					class:border-transparent={selectedColor !== color.value}
					style="background: {color.value}"
					title={color.label}
					aria-label="选择{color.label}批注"
					aria-pressed={selectedColor === color.value}
				></button>
			{/each}
		</div>

		<label for={noteId} class="sr-only">批注内容</label>
		<textarea
			id={noteId}
			bind:value={note}
			placeholder="写点笔记... (可选)"
			class="w-full resize-none rounded-lg border border-ink-700/50 bg-ink-800 px-3 py-2 text-sm text-ink-200 placeholder-ink-500 outline-none focus:border-accent-500/30"
			rows="3"
		></textarea>

		<!-- Actions -->
		<div class="mt-3 flex justify-end gap-2">
			<button
				type="button"
				onclick={onclose}
				class="rounded-lg px-3 py-1.5 text-sm text-ink-400 transition-colors hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300"
			>
				取消
			</button>
			<button
				type="button"
				onclick={() => onsave(note, selectedColor)}
				class="rounded-lg bg-accent-500 px-4 py-1.5 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300 focus-visible:ring-offset-2 focus-visible:ring-offset-ink-900"
			>
				保存
			</button>
		</div>
	</div>
</div>
