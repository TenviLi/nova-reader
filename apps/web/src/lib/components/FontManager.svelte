<script lang="ts">
	import { Upload, Trash2, Type } from 'lucide-svelte';
	import { toast } from 'svelte-sonner';

	let fonts = $state<Array<{ name: string; file: string; active: boolean }>>([]);
	let uploading = $state(false);

	// Load saved custom fonts
	$effect(() => {
		const saved = localStorage.getItem('nova-custom-fonts');
		if (saved) {
			fonts = JSON.parse(saved);
			// Register fonts in document
			fonts.forEach(registerFont);
		}
	});

	function registerFont(font: { name: string; file: string }) {
		const fontFace = new FontFace(font.name, `url(${font.file})`);
		fontFace.load().then((loaded) => {
			document.fonts.add(loaded);
		}).catch(() => {
			console.warn(`Failed to load font: ${font.name}`);
		});
	}

	async function handleUpload(e: Event) {
		const input = e.target as HTMLInputElement;
		const file = input.files?.[0];
		if (!file) return;

		if (!file.name.match(/\.(ttf|otf|woff|woff2)$/i)) {
			toast.error('请上传 TTF、OTF、WOFF 或 WOFF2 格式的字体文件');
			return;
		}

		uploading = true;
		try {
			const reader = new FileReader();
			const dataUrl = await new Promise<string>((resolve) => {
				reader.onload = () => resolve(reader.result as string);
				reader.readAsDataURL(file);
			});

			const fontName = file.name.replace(/\.[^.]+$/, '');
			const newFont = { name: fontName, file: dataUrl, active: false };

			registerFont(newFont);
			fonts = [...fonts, newFont];
			saveFonts();
			toast.success(`字体 "${fontName}" 已添加`);
		} catch {
			toast.error('字体加载失败');
		} finally {
			uploading = false;
			input.value = '';
		}
	}

	function removeFont(index: number) {
		fonts = fonts.filter((_, i) => i !== index);
		saveFonts();
	}

	function toggleFont(index: number) {
		fonts[index].active = !fonts[index].active;
		fonts = [...fonts];
		saveFonts();
	}

	function saveFonts() {
		localStorage.setItem('nova-custom-fonts', JSON.stringify(fonts));
	}

	export function getActiveFonts(): string[] {
		return fonts.filter(f => f.active).map(f => f.name);
	}
</script>

<div class="space-y-4">
	<div class="flex items-center justify-between">
		<h3 class="text-sm font-medium text-ink-200">自定义字体</h3>
		<label class="flex items-center gap-1.5 text-xs text-accent-400 hover:text-accent-300 cursor-pointer">
			<Upload size={14} />
			上传字体
			<input type="file" accept=".ttf,.otf,.woff,.woff2" class="hidden" onchange={handleUpload} />
		</label>
	</div>

	{#if fonts.length === 0}
		<p class="text-xs text-ink-500 text-center py-4">
			暂无自定义字体，点击上方按钮上传
		</p>
	{:else}
		<div class="space-y-2">
			{#each fonts as font, i}
				<div class="flex items-center justify-between rounded-lg border border-ink-700 bg-ink-800/50 px-3 py-2.5">
					<div class="flex items-center gap-2">
						<Type size={14} class="text-ink-400" />
						<span class="text-sm text-ink-200" style="font-family: '{font.name}'">{font.name}</span>
					</div>
					<div class="flex items-center gap-2">
						<button
							class="text-xs px-2 py-0.5 rounded {font.active ? 'bg-accent-600 text-white' : 'bg-ink-700 text-ink-400'}"
							onclick={() => toggleFont(i)}
						>
							{font.active ? '使用中' : '启用'}
						</button>
						<button
							class="text-ink-500 hover:text-red-400"
							onclick={() => removeFont(i)}
						>
							<Trash2 size={14} />
						</button>
					</div>
				</div>
			{/each}
		</div>
	{/if}

	<!-- Font preview -->
	{#if fonts.some(f => f.active)}
		<div class="rounded-lg border border-ink-700 bg-ink-800/30 p-3">
			<p class="text-xs text-ink-500 mb-2">预览</p>
			<p class="text-lg text-ink-200" style="font-family: {fonts.filter(f => f.active).map(f => `'${f.name}'`).join(', ')}">
				春风得意马蹄疾，一日看尽长安花。The quick brown fox jumps over the lazy dog.
			</p>
		</div>
	{/if}
</div>
