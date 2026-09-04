/**
 * Animated Terminal Demo System
 *
 * Provides smooth, professional terminal animations with:
 * - Step-by-step reveal with configurable timing
 * - Pause/resume functionality with full overlay
 * - Replay support
 * - Auto-scrolling as content appears
 * - Cycling between multiple demos
 */

class TerminalDemo {
    constructor(element) {
        this.element = element;
        this.content = element.querySelector('.session-content');
        this.steps = element.querySelectorAll('.session-step, .session-divider');
        this.pauseOverlay = element.querySelector('.pause-overlay');
        this.pauseBtn = element.querySelector('.pause-btn');
        this.replayBtn = element.querySelector('.replay-btn');

        this.isPaused = false;
        this.isStarted = false;
        this.currentStep = 0;
        this.timeouts = [];

        this.init();
    }

    init() {
        // Set up button handlers
        if (this.pauseBtn) {
            this.pauseBtn.addEventListener('click', () => this.togglePause());
        }
        if (this.replayBtn) {
            this.replayBtn.addEventListener('click', () => this.replay());
        }

        // Resume button in overlay
        const resumeBtn = this.pauseOverlay?.querySelector('.resume-btn');
        if (resumeBtn) {
            resumeBtn.addEventListener('click', () => this.resume());
        }

        // Start animation when visible (Intersection Observer)
        const observer = new IntersectionObserver((entries) => {
            entries.forEach(entry => {
                if (entry.isIntersecting && !this.isStarted) {
                    this.start();
                }
            });
        }, { threshold: 0.3 });

        observer.observe(this.element);
    }

    start() {
        if (this.isStarted) return;
        this.isStarted = true;

        // Animate each step with its delay
        this.steps.forEach((step, index) => {
            const delay = parseFloat(getComputedStyle(step).getPropertyValue('--delay')) * 1000 || (index * 1500);

            const timeout = setTimeout(() => {
                if (!this.isPaused) {
                    step.classList.add('animate');
                    this.currentStep = index;
                    this.scrollToStep(step);
                }
            }, delay);

            this.timeouts.push(timeout);
        });
    }

    scrollToStep(step) {
        if (!this.content) return;

        const targetScroll = step.offsetTop - this.content.offsetTop - 20;
        this.smoothScrollTo(this.content, targetScroll, 500);
    }

    smoothScrollTo(element, target, duration) {
        const start = element.scrollTop;
        const distance = target - start;
        let startTime = null;

        const animation = (currentTime) => {
            if (!startTime) startTime = currentTime;
            const elapsed = currentTime - startTime;
            const progress = Math.min(elapsed / duration, 1);

            // Ease out cubic
            const ease = 1 - Math.pow(1 - progress, 3);
            element.scrollTop = start + distance * ease;

            if (progress < 1) {
                requestAnimationFrame(animation);
            }
        };

        requestAnimationFrame(animation);
    }

    togglePause() {
        if (this.isPaused) {
            this.resume();
        } else {
            this.pause();
        }
    }

    pause() {
        this.isPaused = true;
        this.element.classList.add('paused');

        if (this.pauseBtn) {
            this.pauseBtn.textContent = 'Resume';
            this.pauseBtn.classList.add('active');
        }

        if (this.pauseOverlay) {
            this.pauseOverlay.classList.add('visible');
        }

        // Clear pending timeouts
        this.timeouts.forEach(t => clearTimeout(t));
        this.timeouts = [];
    }

    resume() {
        this.isPaused = false;
        this.element.classList.remove('paused');

        if (this.pauseBtn) {
            this.pauseBtn.textContent = 'Pause';
            this.pauseBtn.classList.remove('active');
        }

        if (this.pauseOverlay) {
            this.pauseOverlay.classList.remove('visible');
        }

        // Resume animations from current step
        this.steps.forEach((step, index) => {
            if (index > this.currentStep && !step.classList.contains('animate')) {
                const baseDelay = parseFloat(getComputedStyle(step).getPropertyValue('--delay')) * 1000 || (index * 1500);
                const adjustedDelay = baseDelay - (this.currentStep * 1500);

                const timeout = setTimeout(() => {
                    if (!this.isPaused) {
                        step.classList.add('animate');
                        this.currentStep = index;
                        this.scrollToStep(step);
                    }
                }, Math.max(0, adjustedDelay));

                this.timeouts.push(timeout);
            }
        });
    }

    replay() {
        // Clear everything
        this.timeouts.forEach(t => clearTimeout(t));
        this.timeouts = [];
        this.isPaused = false;
        this.isStarted = false;
        this.currentStep = 0;

        this.element.classList.remove('paused');

        if (this.pauseBtn) {
            this.pauseBtn.textContent = 'Pause';
            this.pauseBtn.classList.remove('active');
        }

        if (this.pauseOverlay) {
            this.pauseOverlay.classList.remove('visible');
        }

        // Reset all steps
        this.steps.forEach(step => {
            step.classList.remove('animate');
        });

        // Scroll to top
        if (this.content) {
            this.content.scrollTop = 0;
        }

        // Restart after brief delay
        setTimeout(() => this.start(), 100);
    }
}

/**
 * Cycling Demo Carousel
 * Cycles through multiple terminal demos automatically
 */
class DemoCarousel {
    constructor(element) {
        this.element = element;
        this.demos = Array.from(element.querySelectorAll('.session-demo'));
        this.indicators = element.querySelectorAll('.indicator-dot');
        this.currentIndex = 0;
        this.cycleInterval = null;
        this.isPaused = false;

        this.init();
    }

    init() {
        if (this.demos.length === 0) return;

        // Hide all but first
        this.demos.forEach((demo, i) => {
            demo.style.display = i === 0 ? 'block' : 'none';
        });

        // Update indicators
        this.updateIndicators();

        // Indicator click handlers
        this.indicators.forEach((dot, i) => {
            dot.addEventListener('click', () => this.goTo(i));
        });

        // Calculate total duration of first demo and set cycle
        this.startCycling();
    }

    startCycling() {
        // Get the max delay from all steps in current demo
        const currentDemo = this.demos[this.currentIndex];
        const steps = currentDemo.querySelectorAll('.session-step, .session-divider');
        let maxDelay = 0;

        steps.forEach(step => {
            const delay = parseFloat(getComputedStyle(step).getPropertyValue('--delay')) * 1000 || 0;
            maxDelay = Math.max(maxDelay, delay);
        });

        // Add buffer time after last step
        const cycleDuration = maxDelay + 5000;

        this.cycleInterval = setInterval(() => {
            if (!this.isPaused) {
                this.next();
            }
        }, cycleDuration);
    }

    next() {
        this.goTo((this.currentIndex + 1) % this.demos.length);
    }

    goTo(index) {
        if (index === this.currentIndex) return;

        // Hide current
        this.demos[this.currentIndex].style.display = 'none';

        // Show next
        this.currentIndex = index;
        this.demos[index].style.display = 'block';

        // Replay the demo
        const demoInstance = this.demos[index]._terminalDemo;
        if (demoInstance) {
            demoInstance.replay();
        }

        this.updateIndicators();
    }

    updateIndicators() {
        this.indicators.forEach((dot, i) => {
            dot.classList.toggle('active', i === this.currentIndex);
        });
    }

    pause() {
        this.isPaused = true;
    }

    resume() {
        this.isPaused = false;
    }
}

// Initialize all demos on page load
document.addEventListener('DOMContentLoaded', () => {
    // Initialize individual demos
    document.querySelectorAll('.session-demo').forEach(demo => {
        demo._terminalDemo = new TerminalDemo(demo);
    });

    // Initialize carousels
    document.querySelectorAll('.demo-carousel').forEach(carousel => {
        new DemoCarousel(carousel);
    });
});

// Export for manual usage
window.TerminalDemo = TerminalDemo;
window.DemoCarousel = DemoCarousel;
