export type ToastType = 'success' | 'error' | 'info' | 'warning';

interface Toast {
	id: string;
	type: ToastType;
	title: string;
	message?: string;
	duration: number;
}

class ToastStore {
	toasts = $state<Toast[]>([]);

	private counter = 0;

	push(type: ToastType, title: string, message?: string, duration = 4000) {
		const id = `toast-${++this.counter}`;
		const toast: Toast = { id, type, title, message, duration };
		this.toasts = [...this.toasts, toast];

		if (duration > 0) {
			setTimeout(() => this.dismiss(id), duration);
		}
	}

	dismiss(id: string) {
		this.toasts = this.toasts.filter(t => t.id !== id);
	}

	success(title: string, message?: string) {
		this.push('success', title, message);
	}

	error(title: string, message?: string) {
		this.push('error', title, message, 6000);
	}

	info(title: string, message?: string) {
		this.push('info', title, message);
	}

	warning(title: string, message?: string) {
		this.push('warning', title, message, 5000);
	}
}

export const toastStore = new ToastStore();
