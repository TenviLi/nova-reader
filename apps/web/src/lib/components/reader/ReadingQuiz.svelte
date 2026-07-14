<script lang="ts">
	import { GraduationCap, Check, X, RotateCcw, Loader2, Trophy } from 'lucide-svelte';

	interface QuizQuestion {
		id: string;
		question: string;
		options: string[];
		correct_index: number;
		explanation: string;
		chapter_ref?: number;
	}

	interface Props {
		bookId: string;
		questions?: QuizQuestion[];
		loading?: boolean;
		onGenerate?: (chapterRange?: [number, number]) => void;
	}

	let { bookId, questions = [], loading = false, onGenerate }: Props = $props();

	let currentIndex = $state(0);
	let answers = $state<Record<string, number>>({});
	let showExplanation = $state(false);
	let quizComplete = $state(false);

	let currentQuestion = $derived(questions[currentIndex]);
	let selectedAnswer = $derived(currentQuestion ? answers[currentQuestion.id] : undefined);
	let isCorrect = $derived(
		currentQuestion && selectedAnswer !== undefined
			? selectedAnswer === currentQuestion.correct_index
			: undefined
	);
	let score = $derived(
		questions.reduce((acc, q) => acc + (answers[q.id] === q.correct_index ? 1 : 0), 0)
	);

	function selectAnswer(index: number) {
		if (!currentQuestion || selectedAnswer !== undefined) return;
		answers = { ...answers, [currentQuestion.id]: index };
		showExplanation = true;
	}

	function nextQuestion() {
		showExplanation = false;
		if (currentIndex < questions.length - 1) {
			currentIndex++;
		} else {
			quizComplete = true;
		}
	}

	function resetQuiz() {
		currentIndex = 0;
		answers = {};
		showExplanation = false;
		quizComplete = false;
	}
</script>

<div class="rounded-xl border border-ink-100 bg-white p-6 dark:border-ink-700 dark:bg-ink-900">
	<!-- Header -->
	<div class="mb-4 flex items-center justify-between">
		<div class="flex items-center gap-2">
			<GraduationCap class="h-5 w-5 text-accent-500" />
			<h3 class="text-lg font-semibold text-ink-800 dark:text-ink-200">阅读理解</h3>
		</div>
		{#if questions.length === 0}
			<button
				class="rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-accent-600 disabled:opacity-50"
				onclick={() => onGenerate?.()}
				disabled={loading}
			>
				{#if loading}
					<Loader2 class="inline h-4 w-4 animate-spin" /> 生成中...
				{:else}
					生成题目
				{/if}
			</button>
		{/if}
	</div>

	{#if questions.length === 0 && !loading}
		<div class="py-12 text-center text-ink-400">
			<GraduationCap class="mx-auto mb-3 h-12 w-12 opacity-30" />
			<p class="text-sm">AI 将根据已读章节生成理解测试题</p>
			<p class="mt-1 text-xs text-ink-300">帮助加深对故事情节、角色和细节的理解</p>
		</div>
	{:else if quizComplete}
		<!-- Quiz complete -->
		<div class="py-8 text-center">
			<Trophy class="mx-auto mb-4 h-12 w-12 text-amber-500" />
			<h4 class="text-xl font-bold text-ink-800 dark:text-ink-200">
				{score} / {questions.length}
			</h4>
			<p class="mt-2 text-sm text-ink-500">
				{#if score === questions.length}
					完美！你对这本书了如指掌 🎉
				{:else if score >= questions.length * 0.7}
					很不错！大部分细节都记住了
				{:else}
					可以再回顾一下相关章节
				{/if}
			</p>
			<div class="mt-4 flex justify-center gap-3">
				<button
					class="rounded-lg bg-ink-100 px-4 py-2 text-sm text-ink-600 hover:bg-ink-200 dark:bg-ink-700 dark:text-ink-400 dark:hover:bg-ink-600"
					onclick={resetQuiz}
				>
					<RotateCcw class="mr-1 inline h-4 w-4" /> 重来
				</button>
				<button
					class="rounded-lg bg-accent-500 px-4 py-2 text-sm text-white hover:bg-accent-600"
					onclick={() => onGenerate?.()}
				>
					新一组
				</button>
			</div>
		</div>
	{:else if currentQuestion}
		<!-- Progress -->
		<div class="mb-4 flex items-center gap-3">
			<div class="h-1.5 flex-1 overflow-hidden rounded-full bg-ink-100 dark:bg-ink-700">
				<div
					class="h-full rounded-full bg-accent-500 transition-all"
					style="width: {((currentIndex + 1) / questions.length) * 100}%"
				></div>
			</div>
			<span class="text-xs text-ink-400">{currentIndex + 1}/{questions.length}</span>
		</div>

		<!-- Question -->
		<div class="mb-4">
			<p class="text-base font-medium leading-relaxed text-ink-800 dark:text-ink-200">
				{currentQuestion.question}
			</p>
			{#if currentQuestion.chapter_ref !== undefined}
				<span class="mt-1 inline-block text-xs text-ink-400">
					— 来自第{currentQuestion.chapter_ref + 1}章
				</span>
			{/if}
		</div>

		<!-- Options -->
		<div class="space-y-2">
			{#each currentQuestion.options as option, idx}
				{@const isSelected = selectedAnswer === idx}
				{@const isCorrectOption = idx === currentQuestion.correct_index}
				<button
					class="flex w-full items-center gap-3 rounded-lg border p-3 text-left text-sm transition-all {
						selectedAnswer === undefined
							? 'border-ink-200 hover:border-accent-300 hover:bg-accent-50 dark:border-ink-600 dark:hover:border-accent-700 dark:hover:bg-accent-900/20'
							: isSelected && isCorrectOption
								? 'border-green-300 bg-green-50 dark:border-green-700 dark:bg-green-900/20'
								: isSelected && !isCorrectOption
									? 'border-red-300 bg-red-50 dark:border-red-700 dark:bg-red-900/20'
									: isCorrectOption
										? 'border-green-200 bg-green-50/50 dark:border-green-800 dark:bg-green-900/10'
										: 'border-ink-100 opacity-50 dark:border-ink-700'
					}"
					onclick={() => selectAnswer(idx)}
					disabled={selectedAnswer !== undefined}
				>
					<span class="flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-full border text-xs font-medium {
						isSelected && isCorrectOption ? 'border-green-500 bg-green-500 text-white' :
						isSelected && !isCorrectOption ? 'border-red-500 bg-red-500 text-white' :
						'border-ink-300 text-ink-500 dark:border-ink-500'
					}">
						{#if selectedAnswer !== undefined && isSelected}
							{#if isCorrectOption}
								<Check class="h-3.5 w-3.5" />
							{:else}
								<X class="h-3.5 w-3.5" />
							{/if}
						{:else}
							{String.fromCharCode(65 + idx)}
						{/if}
					</span>
					<span class="text-ink-700 dark:text-ink-300">{option}</span>
				</button>
			{/each}
		</div>

		<!-- Explanation -->
		{#if showExplanation}
			<div class="mt-4 rounded-lg border border-accent-100 bg-accent-50/50 p-3 dark:border-accent-800/30 dark:bg-accent-900/10">
				<p class="text-sm text-ink-600 dark:text-ink-400">
					<span class="font-medium text-accent-600 dark:text-accent-400">解析：</span>
					{currentQuestion.explanation}
				</p>
			</div>
			<button
				class="mt-4 w-full rounded-lg bg-accent-500 py-2.5 text-sm font-medium text-white transition-colors hover:bg-accent-600"
				onclick={nextQuestion}
			>
				{currentIndex < questions.length - 1 ? '下一题' : '查看结果'}
			</button>
		{/if}
	{/if}
</div>
