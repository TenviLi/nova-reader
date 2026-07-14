<script lang="ts">
	import { Play, Pause, Square, SkipForward, SkipBack, Volume2, Settings } from 'lucide-svelte';

	interface Props {
		text: string;
		onSentenceChange?: (sentenceIndex: number) => void;
	}

	let { text, onSentenceChange }: Props = $props();

	let playing = $state(false);
	let paused = $state(false);
	let rate = $state(1.0);
	let pitch = $state(1.0);
	let voiceIndex = $state(0);
	let currentSentence = $state(0);
	let showSettings = $state(false);
	let voices = $state<SpeechSynthesisVoice[]>([]);
	let utterance: SpeechSynthesisUtterance | null = null;
	const controlsId = $props.id();

	// Split text into sentences for tracking
	let sentences = $derived(
		text.split(/(?<=[。！？.!?\n])\s*/).filter(s => s.trim().length > 0)
	);
	let hasSentences = $derived(sentences.length > 0);

	$effect(() => {
		if (typeof window !== 'undefined' && 'speechSynthesis' in window) {
			const loadVoices = () => {
				const available = speechSynthesis.getVoices();
				// Prefer Chinese voices, then Japanese, then English
				voices = available.sort((a, b) => {
					const langPriority = (lang: string) => {
						if (lang.startsWith('zh')) return 0;
						if (lang.startsWith('ja')) return 1;
						if (lang.startsWith('en')) return 2;
						return 3;
					};
					return langPriority(a.lang) - langPriority(b.lang);
				});
			};
			loadVoices();
			speechSynthesis.onvoiceschanged = loadVoices;
		}
	});

	function speak() {
		if (!('speechSynthesis' in window)) return;

		if (paused) {
			speechSynthesis.resume();
			paused = false;
			playing = true;
			return;
		}

		stop();
		speakFromSentence(currentSentence);
	}

	function speakFromSentence(index: number) {
		if (index >= sentences.length) {
			stop();
			return;
		}

		currentSentence = index;
		onSentenceChange?.(index);

		utterance = new SpeechSynthesisUtterance(sentences[index]);
		utterance.rate = rate;
		utterance.pitch = pitch;
		if (voices[voiceIndex]) {
			utterance.voice = voices[voiceIndex];
		}

		utterance.onend = () => {
			if (playing && !paused) {
				speakFromSentence(index + 1);
			}
		};

		utterance.onerror = () => {
			stop();
		};

		playing = true;
		speechSynthesis.speak(utterance);
	}

	function pause() {
		speechSynthesis.pause();
		paused = true;
		playing = false;
	}

	function stop() {
		speechSynthesis.cancel();
		playing = false;
		paused = false;
	}

	function skipForward() {
		stop();
		const next = Math.min(currentSentence + 1, sentences.length - 1);
		currentSentence = next;
		speakFromSentence(next);
	}

	function skipBack() {
		stop();
		const prev = Math.max(currentSentence - 1, 0);
		currentSentence = prev;
		speakFromSentence(prev);
	}

	// Cleanup on component destroy
	$effect(() => {
		return () => {
			if (typeof window !== 'undefined' && 'speechSynthesis' in window) {
				speechSynthesis.cancel();
			}
		};
	});
</script>

<div class="flex items-center gap-2 rounded-lg border border-ink-200 bg-white px-3 py-2 dark:border-ink-700 dark:bg-ink-900">
	<!-- Playback controls -->
	<button
		class="rounded p-1.5 text-ink-500 hover:bg-ink-100 dark:hover:bg-ink-700"
		onclick={skipBack}
		title="上一句"
		type="button"
		aria-label="朗读上一句"
		disabled={!hasSentences}
	>
		<SkipBack class="h-4 w-4" />
	</button>

	{#if playing}
		<button
			class="rounded-full bg-accent-500 p-2 text-white hover:bg-accent-600"
			onclick={pause}
			title="暂停"
			type="button"
			aria-label="暂停朗读"
		>
			<Pause class="h-4 w-4" />
		</button>
	{:else}
		<button
			class="rounded-full bg-accent-500 p-2 text-white hover:bg-accent-600"
			onclick={speak}
			title="朗读"
			type="button"
			aria-label="开始朗读"
			disabled={!hasSentences}
		>
			<Play class="h-4 w-4" />
		</button>
	{/if}

	<button
		class="rounded p-1.5 text-ink-500 hover:bg-ink-100 dark:hover:bg-ink-700"
		onclick={stop}
		title="停止"
		type="button"
		aria-label="停止朗读"
	>
		<Square class="h-3.5 w-3.5" />
	</button>

	<button
		class="rounded p-1.5 text-ink-500 hover:bg-ink-100 dark:hover:bg-ink-700"
		onclick={skipForward}
		title="下一句"
		type="button"
		aria-label="朗读下一句"
		disabled={!hasSentences}
	>
		<SkipForward class="h-4 w-4" />
	</button>

	<!-- Progress indicator -->
	<div class="flex-1 text-center text-xs text-ink-400">
		{#if sentences.length > 0}
			{currentSentence + 1} / {sentences.length}
		{/if}
	</div>

	<!-- Speed control -->
	<div class="flex items-center gap-1">
		<Volume2 class="h-3.5 w-3.5 text-ink-400" />
		<span class="text-xs text-ink-500">{rate.toFixed(1)}x</span>
	</div>

	<!-- Settings toggle -->
	<button
		class="rounded p-1.5 text-ink-500 hover:bg-ink-100 dark:hover:bg-ink-700"
		onclick={() => showSettings = !showSettings}
		title="设置"
		type="button"
		aria-label="打开朗读设置"
		aria-expanded={showSettings}
	>
		<Settings class="h-4 w-4" />
	</button>
</div>

<!-- Settings panel -->
{#if showSettings}
	<div class="mt-2 space-y-3 rounded-lg border border-ink-200 bg-white p-3 dark:border-ink-700 dark:bg-ink-900">
		<div class="flex items-center gap-3">
			<label for="{controlsId}-rate" class="w-12 text-xs text-ink-500">语速</label>
			<input
				id="{controlsId}-rate"
				type="range"
				min="0.5"
				max="2.0"
				step="0.1"
				bind:value={rate}
				class="flex-1 accent-accent-500"
			/>
			<span class="w-10 text-right text-xs text-ink-600">{rate.toFixed(1)}x</span>
		</div>
		<div class="flex items-center gap-3">
			<label for="{controlsId}-pitch" class="w-12 text-xs text-ink-500">音调</label>
			<input
				id="{controlsId}-pitch"
				type="range"
				min="0.5"
				max="2.0"
				step="0.1"
				bind:value={pitch}
				class="flex-1 accent-accent-500"
			/>
			<span class="w-10 text-right text-xs text-ink-600">{pitch.toFixed(1)}</span>
		</div>
		{#if voices.length > 0}
			<div class="flex items-center gap-3">
				<label for="{controlsId}-voice" class="w-12 text-xs text-ink-500">语音</label>
				<select
					id="{controlsId}-voice"
					class="flex-1 rounded border border-ink-200 bg-ink-50 px-2 py-1 text-xs dark:border-ink-600 dark:bg-ink-800"
					bind:value={voiceIndex}
				>
					{#each voices as voice, i}
						<option value={i}>{voice.name} ({voice.lang})</option>
					{/each}
				</select>
			</div>
		{/if}
	</div>
{/if}
