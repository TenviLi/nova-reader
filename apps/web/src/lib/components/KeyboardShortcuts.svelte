<script lang="ts">
	import { goto } from '$app/navigation';

	let open = $state(false);
	let pendingGChord = $state(false);

	const shortcuts = [
		{ category: '全局', items: [
			{ keys: ['⌘', 'K'], description: '打开命令面板' },
			{ keys: ['?'], description: '显示快捷键帮助' },
			{ keys: ['⌘', '/'], description: '全局搜索' },
			{ keys: ['G', 'H'], description: '返回主页' },
			{ keys: ['G', 'L'], description: '前往所有书籍' },
			{ keys: ['G', 'M'], description: '前往书库管理' },
			{ keys: ['G', 'S'], description: '前往搜索' },
		]},
		{ category: '阅读器', items: [
			{ keys: ['←'], description: '上一章' },
			{ keys: ['→'], description: '下一章' },
			{ keys: ['⌘', '+'], description: '增大字体' },
			{ keys: ['⌘', '-'], description: '减小字体' },
			{ keys: ['⌘', 'S'], description: '切换侧边栏' },
			{ keys: ['F'], description: '全屏切换' },
			{ keys: ['B'], description: '添加书签' },
			{ keys: ['N'], description: '添加批注' },
			{ keys: ['T'], description: '目录' },
		]},
		{ category: '书库', items: [
			{ keys: ['J'], description: '下一本书' },
			{ keys: ['K'], description: '上一本书' },
			{ keys: ['Enter'], description: '打开选中的书' },
		]},
	];

	function handleGlobalKeydown(e: KeyboardEvent) {
		const targetAcceptsText = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
		const isShortcutHelpKey = e.key === '?' || (e.key === '/' && e.shiftKey);
		if (isShortcutHelpKey && !e.metaKey && !e.ctrlKey && !targetAcceptsText) {
			e.preventDefault();
			open = !open;
		}
		if (e.key === 'Escape' && open) {
			open = false;
		}
		if (e.metaKey || e.ctrlKey || e.altKey || targetAcceptsText) return;

		const key = e.key.toLowerCase();
		if (pendingGChord) {
			pendingGChord = false;
			if (key === 'h') {
				e.preventDefault();
				goto('/');
			} else if (key === 'l') {
				e.preventDefault();
				goto('/library');
			} else if (key === 'm') {
				e.preventDefault();
				goto('/libraries');
			} else if (key === 's') {
				e.preventDefault();
				goto('/search');
			}
			return;
		}
		if (key === 'g') {
			pendingGChord = true;
			window.setTimeout(() => {
				pendingGChord = false;
			}, 900);
		}
	}

	function closeShortcuts() {
		open = false;
	}
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

{#if open}
	<div class="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 backdrop-blur-sm">
		<button
			type="button"
			class="absolute inset-0 cursor-default"
			aria-label="关闭快捷键帮助"
			onclick={closeShortcuts}
		></button>
		<div
			class="relative w-full max-w-lg max-h-[70vh] overflow-y-auto rounded-2xl border border-ink-700 bg-ink-900 p-6 shadow-2xl"
			role="dialog"
			aria-modal="true"
			aria-labelledby="keyboard-shortcuts-title"
			tabindex="-1"
		>
			<div class="flex items-center justify-between mb-5">
				<h2 id="keyboard-shortcuts-title" class="text-lg font-semibold text-ink-100">键盘快捷键</h2>
				<button type="button" class="text-ink-500 hover:text-ink-300 text-sm" onclick={closeShortcuts}>ESC</button>
			</div>

			<div class="space-y-6">
				{#each shortcuts as group}
					<div>
						<h3 class="text-xs font-medium text-ink-400 uppercase tracking-wider mb-2">{group.category}</h3>
						<div class="space-y-1.5">
							{#each group.items as shortcut}
								<div class="flex items-center justify-between py-1">
									<span class="text-sm text-ink-200">{shortcut.description}</span>
									<div class="flex items-center gap-1">
										{#each shortcut.keys as key}
											<kbd class="inline-flex items-center justify-center min-w-[24px] h-6 px-1.5 rounded bg-ink-800 border border-ink-700 text-xs font-mono text-ink-300">
												{key}
											</kbd>
										{/each}
									</div>
								</div>
							{/each}
						</div>
					</div>
				{/each}
			</div>
		</div>
	</div>
{/if}
