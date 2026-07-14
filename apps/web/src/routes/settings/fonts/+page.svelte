<script lang="ts">
import { getErrorMessage } from '$lib/utils';
	import { toast } from 'svelte-sonner';
	import { Upload, Trash2, Type, Eye } from 'lucide-svelte';
	import { browser } from '$app/environment';

	interface FontEntry {
		id: string;
		name: string;
		filename: string;
		size: number;
		url: string;
		addedAt: string;
	}

	let fonts = $state<FontEntry[]>(browser ? JSON.parse(localStorage.getItem('nova_custom_fonts') ?? '[]') : []);
	let previewText = $state('月光透过窗帘缝隙，在地板上画出一道银白色光带。The quick brown fox jumps over the lazy dog.');
	let previewFont = $state<string | null>(null);
	let uploading = $state(false);
	let fileInput: HTMLInputElement;

	function saveFonts() {
		if (!browser) return;
		localStorage.setItem('nova_custom_fonts', JSON.stringify(fonts));
	}

	async function handleUpload(e: Event) {
		const input = e.target as HTMLInputElement;
		const files = input.files;
		if (!files || files.length === 0) return;

		uploading = true;
		for (const file of files) {
			if (!file.name.match(/\.(ttf|otf|woff|woff2)$/i)) {
				toast.error(`不支持的格式: ${file.name}`);
				continue;
			}

			try {
				// Read as data URL for local storage (in production, would upload to server)
				const dataUrl = await readFileAsDataUrl(file);
				const fontName = file.name.replace(/\.(ttf|otf|woff|woff2)$/i, '');

				// Register font face
				const fontFace = new FontFace(fontName, `url(${dataUrl})`);
				await fontFace.load();
				document.fonts.add(fontFace);

				const entry: FontEntry = {
					id: crypto.randomUUID(),
					name: fontName,
					filename: file.name,
					size: file.size,
					url: dataUrl,
					addedAt: new Date().toISOString(),
				};

				fonts = [...fonts, entry];
				saveFonts();
				toast.success(`字体「${fontName}」已添加`);
			} catch (err: unknown) {
				toast.error(`加载失败: ${getErrorMessage(err)}`);
			}
		}
		uploading = false;
		input.value = '';
	}

	function readFileAsDataUrl(file: File): Promise<string> {
		return new Promise((resolve, reject) => {
			const reader = new FileReader();
			reader.onload = () => resolve(reader.result as string);
			reader.onerror = reject;
			reader.readAsDataURL(file);
		});
	}

	function removeFont(id: string) {
		fonts = fonts.filter(f => f.id !== id);
		saveFonts();
		toast.success('字体已移除');
	}

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / 1048576).toFixed(1)} MB`;
	}

	// Load registered fonts on mount
	$effect(() => {
		if (!browser) return;
		for (const font of fonts) {
			try {
				const fontFace = new FontFace(font.name, `url(${font.url})`);
				fontFace.load().then(f => document.fonts.add(f));
			} catch { /* skip corrupted entries */ }
		}
	});
</script>

<svelte:head>
	<title>字体管理 — Nova Reader</title>
</svelte:head>

<div class="mx-auto max-w-[1000px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">字体管理</h1>
			<p class="mt-1 text-sm text-ink-400">上传自定义字体用于阅读器</p>
		</div>
		<button
			onclick={() => fileInput.click()}
			disabled={uploading}
			class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-ink-950 hover:bg-accent-400 transition-colors disabled:opacity-50"
		>
			<Upload size={16} strokeWidth={2} />
			{uploading ? '上传中...' : '上传字体'}
		</button>
		<input
			bind:this={fileInput}
			type="file"
			accept=".ttf,.otf,.woff,.woff2"
			multiple
			class="hidden"
			onchange={handleUpload}
		/>
	</div>

	<!-- Preview area -->
	<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
		<div class="flex items-center gap-2 mb-3">
			<Eye size={16} class="text-ink-400" />
			<h3 class="text-sm font-medium text-ink-200">实时预览</h3>
		</div>
		<textarea
			bind:value={previewText}
			class="w-full rounded-lg border border-ink-700/50 bg-ink-950/50 p-3 text-sm text-ink-300 outline-none resize-none focus:border-accent-500/30"
			rows="2"
			placeholder="输入预览文本..."
		></textarea>
		{#if previewFont}
			<div
				class="mt-3 rounded-lg bg-ink-950/50 p-4 text-lg text-ink-100 leading-relaxed"
				style="font-family: '{previewFont}', serif;"
			>
				{previewText}
			</div>
		{:else}
			<p class="mt-2 text-xs text-ink-500">选择一个字体进行预览</p>
		{/if}
	</div>

	<!-- Font list -->
	{#if fonts.length === 0}
		<div class="text-center py-16">
			<Type size={40} strokeWidth={1} class="text-ink-600 mx-auto mb-4" />
			<p class="text-ink-400">还没有自定义字体</p>
			<p class="mt-1 text-sm text-ink-500">支持 .ttf, .otf, .woff, .woff2 格式</p>
		</div>
	{:else}
		<div class="space-y-2">
			{#each fonts as font (font.id)}
				<div class="flex items-center gap-4 rounded-xl border border-ink-800/50 bg-ink-900/30 p-4 transition-all hover:border-ink-700/50">
					<!-- Font preview character -->
					<div
						class="flex h-12 w-12 items-center justify-center rounded-lg bg-ink-800/50 text-xl text-ink-200"
						style="font-family: '{font.name}', serif;"
					>
						Aa
					</div>

					<!-- Info -->
					<div class="flex-1 min-w-0">
						<h4 class="text-sm font-medium text-ink-100">{font.name}</h4>
						<p class="text-xs text-ink-500 mt-0.5">{font.filename} · {formatSize(font.size)}</p>
					</div>

					<!-- Actions -->
					<button
						onclick={() => previewFont = previewFont === font.name ? null : font.name}
						class="rounded-lg px-3 py-1.5 text-xs transition-colors {previewFont === font.name ? 'bg-accent-500/10 text-accent-400' : 'text-ink-400 hover:text-ink-200 hover:bg-ink-800/50'}"
					>
						预览
					</button>
					<button
						onclick={() => removeFont(font.id)}
						class="rounded-lg p-2 text-ink-500 hover:text-red-400 hover:bg-red-500/10 transition-colors"
					>
						<Trash2 size={14} strokeWidth={2} />
					</button>
				</div>
			{/each}
		</div>
	{/if}
</div>
