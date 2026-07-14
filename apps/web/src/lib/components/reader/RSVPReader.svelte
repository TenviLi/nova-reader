<script lang="ts">
	import { Play, Pause, SkipForward, SkipBack, Settings, Gauge } from 'lucide-svelte';

	interface Props {
		/** Text content to display word-by-word */
		text: string;
		/** Words per minute (default: 300) */
		initialWpm?: number;
		onComplete?: () => void;
	}

	let { text, initialWpm = 300, onComplete }: Props = $props();

	// State
	let playing = $state(false);
	let selectedWpm = $state<number | null>(null);
	let currentWordIndex = $state(0);
	let showSettings = $state(false);
	let chunkSize = $state(1); // 1 = word-by-word, 2-3 = phrase mode
	let highlightPivot = $state(true); // highlight the ORP (Optimal Recognition Point)

	// Derived
	let words = $derived(text.split(/\s+/).filter(w => w.length > 0));
	let currentWord = $derived(
		words.slice(currentWordIndex, currentWordIndex + chunkSize).join(' ')
	);
	let wpm = $derived(selectedWpm ?? initialWpm);
	let progress = $derived(words.length > 0 ? (currentWordIndex / words.length) * 100 : 0);
	let msPerWord = $derived(60000 / wpm);
	let estimatedTimeLeft = $derived(
		Math.ceil(((words.length - currentWordIndex) / wpm) * 60)
	);

	// ORP: The pivot letter (roughly 1/3 into the word)
	let pivotIndex = $derived(Math.max(0, Math.floor(currentWord.length * 0.33) - 1));

	let interval: ReturnType<typeof setInterval> | null = null;

	function start() {
		if (currentWordIndex >= words.length) {
			currentWordIndex = 0;
		}
		playing = true;
		interval = setInterval(() => {
			if (currentWordIndex + chunkSize >= words.length) {
				stop();
				onComplete?.();
				return;
			}
			currentWordIndex += chunkSize;
		}, msPerWord * chunkSize);
	}

	function stop() {
		playing = false;
		if (interval) {
			clearInterval(interval);
			interval = null;
		}
	}

	function toggle() {
		if (playing) stop();
		else start();
	}

	function skipForward() {
		currentWordIndex = Math.min(words.length - 1, currentWordIndex + 10);
	}

	function skipBack() {
		currentWordIndex = Math.max(0, currentWordIndex - 10);
	}

	function adjustWpm(delta: number) {
		selectedWpm = Math.max(100, Math.min(1000, wpm + delta));
		if (playing) {
			stop();
			start(); // restart with new speed
		}
	}

	// Cleanup on destroy
	$effect(() => {
		return () => {
			if (interval) clearInterval(interval);
		};
	});
</script>

<div class="flex flex-col items-center rounded-xl border border-ink-100 bg-ink-950 p-8 dark:border-ink-700">
	<!-- Display area -->
	<div class="relative mb-8 flex h-24 w-full max-w-lg items-center justify-center">
		<!-- Pivot marker -->
		<div class="absolute top-0 left-1/2 h-2 w-px -translate-x-1/2 bg-red-500"></div>
		<div class="absolute bottom-0 left-1/2 h-2 w-px -translate-x-1/2 bg-red-500"></div>

		<!-- Word display -->
		{#if currentWord}
			<span class="font-mono text-3xl font-bold tracking-wide text-white">
				{#if highlightPivot}
					<span class="text-ink-400">{currentWord.slice(0, pivotIndex)}</span><span class="text-red-400">{currentWord[pivotIndex]}</span><span class="text-ink-400">{currentWord.slice(pivotIndex + 1)}</span>
				{:else}
					{currentWord}
				{/if}
			</span>
		{:else}
			<span class="text-lg text-ink-500">按下播放开始速读</span>
		{/if}
	</div>

	<!-- Progress bar -->
	<div class="mb-6 w-full max-w-lg">
		<div class="h-1 w-full overflow-hidden rounded-full bg-ink-800">
			<div
				class="h-full rounded-full bg-accent-500 transition-all"
				style="width: {progress}%"
			></div>
		</div>
		<div class="mt-1 flex justify-between text-xs text-ink-500">
			<span>{currentWordIndex}/{words.length} 词</span>
			<span>剩余 {estimatedTimeLeft}s</span>
		</div>
	</div>

	<!-- Controls -->
	<div class="flex items-center gap-4">
		<button
			class="flex h-10 w-10 items-center justify-center rounded-full text-ink-400 transition-colors hover:bg-ink-800 hover:text-white"
			onclick={skipBack}
			type="button"
			aria-label="后退 10 个词"
		>
			<SkipBack class="h-5 w-5" />
		</button>

		<button
			class="flex h-14 w-14 items-center justify-center rounded-full bg-accent-500 text-white shadow-lg transition-transform hover:scale-105"
			onclick={toggle}
			type="button"
			aria-label={playing ? '暂停速读' : '开始速读'}
		>
			{#if playing}
				<Pause class="h-6 w-6" />
			{:else}
				<Play class="ml-0.5 h-6 w-6" />
			{/if}
		</button>

		<button
			class="flex h-10 w-10 items-center justify-center rounded-full text-ink-400 transition-colors hover:bg-ink-800 hover:text-white"
			onclick={skipForward}
			type="button"
			aria-label="前进 10 个词"
		>
			<SkipForward class="h-5 w-5" />
		</button>
	</div>

	<!-- Speed controls -->
	<div class="mt-6 flex items-center gap-4">
		<button
			class="rounded-lg border border-ink-700 px-3 py-1.5 text-sm text-ink-400 hover:bg-ink-800"
			onclick={() => adjustWpm(-50)}
			type="button"
		>
			-50
		</button>
		<div class="flex items-center gap-1.5">
			<Gauge class="h-4 w-4 text-ink-500" />
			<span class="font-mono text-lg font-bold text-white">{wpm}</span>
			<span class="text-xs text-ink-500">WPM</span>
		</div>
		<button
			class="rounded-lg border border-ink-700 px-3 py-1.5 text-sm text-ink-400 hover:bg-ink-800"
			onclick={() => adjustWpm(50)}
			type="button"
		>
			+50
		</button>
	</div>

	<!-- Settings toggle -->
	<button
		class="mt-4 flex items-center gap-1.5 text-xs text-ink-500 hover:text-ink-300"
		onclick={() => showSettings = !showSettings}
		type="button"
		aria-expanded={showSettings}
	>
		<Settings class="h-3.5 w-3.5" />
		设置
	</button>

	{#if showSettings}
		<div class="mt-3 w-full max-w-sm space-y-3 rounded-lg border border-ink-700 bg-ink-900 p-4">
			<label class="flex items-center justify-between text-sm text-ink-400">
				<span>短语模式 (每次显示词数)</span>
				<select
					bind:value={chunkSize}
					class="rounded border border-ink-600 bg-ink-800 px-2 py-1 text-sm text-white"
				>
					<option value={1}>1 词</option>
					<option value={2}>2 词</option>
					<option value={3}>3 词</option>
				</select>
			</label>
			<label class="flex items-center justify-between text-sm text-ink-400">
				<span>ORP 高亮 (最佳识别点)</span>
				<input type="checkbox" bind:checked={highlightPivot} class="rounded" />
			</label>
		</div>
	{/if}
</div>
