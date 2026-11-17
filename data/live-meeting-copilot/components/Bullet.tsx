import { productionLogger } from '../../lib/productionLogger'
import axios from "axios";
import { ArrowUp } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import { Socket } from "socket.io-client";
import { useTranslation } from "../../hooks/useTranslation";
import { baseURL } from "../../lib/baseURL";
import UpgradeNow from "./UpgradeNow";

const isWindows = typeof window !== 'undefined' && window.navigator.platform.toLowerCase().includes('win');

const ThinkingAnimation = () => {
  const [dots, setDots] = useState('.');
  const { t } = useTranslation();

  useEffect(() => {
    const interval = setInterval(() => {
      setDots(prev => {
        if (prev === '...') return '.';
        return prev + '.';
      });
    }, 500);

    return () => clearInterval(interval);
  }, []);

  return (
    <span className="text-sm text-gray-300">
      {t('bullet.thinking')}{dots}
    </span>
  );
};

interface Message {
  role: 'user' | 'assistant';
  content: string;
  timestamp: Date;
  speechData?: {
    speechText: string;
    pronunciation: string;
    language: string;
  };
  isMemoConfirmation?: boolean;
  isSummary?: boolean;
  isExplanation?: boolean;
  summaryId?: string; // For summaries: unique ID, for explanations: links to related summary
  speaker?: string;
}

interface BulletProps {
  show: boolean;
  socketRef: React.MutableRefObject<Socket | null>;
  isToggleOn: boolean;
  setIsToggleOn: (value: boolean) => void;
  onOpenChange?: (isOpen: boolean) => void;
  onHeightChange?: (height: number) => void;
  messages: Message[];
  setMessages: React.Dispatch<React.SetStateAction<Message[]>>;
  userId?: string;
  transcriptData: any[];
  input: string;
  setInput: React.Dispatch<React.SetStateAction<string>>;
  notepad: string;
  setNotepad: React.Dispatch<React.SetStateAction<string>>;
  prompt3?: string;
  prompt4?: string;
  spokenLanguage: string;
  translatedLanguage: string;
  onNavigateToMemos?: () => void;
  autoFocusInput?: boolean;
}

interface CreditUsage {
  used: number;
  limit: number;
}

const Bullet = ({
  show,
  socketRef,
  isToggleOn,
  setIsToggleOn,
  onOpenChange,
  onHeightChange,
  messages,
  setMessages,
  userId,
  transcriptData,
  input,
  setInput,
  notepad,
  setNotepad,
  prompt3,
  prompt4,
  spokenLanguage,
  translatedLanguage,
  onNavigateToMemos,
  autoFocusInput = true,
}: BulletProps) => {
  // Initialize open state based on Real-time Summary toggle on app reload
  const [open, setOpen] = useState(() => {
    // If Real-time Summary is ON, start with panel open
    return isToggleOn;
  });
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [creditUsage, setCreditUsage] = useState<CreditUsage | null>(null);
  const [socketId, setSocketId] = useState<string | null>(null);
  const [dynamicHeight, setDynamicHeight] = useState(() => {
    const savedHeight = localStorage.getItem('askMeetingCopilotHeight');
    return savedHeight ? parseInt(savedHeight) : 50;
  });
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const pillSubmitRef = useRef<(question: string) => void>(() => {});
  const promptButtonsRef = useRef<HTMLDivElement>(null);
  const resizeHandleRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [dragStartY, setDragStartY] = useState(0);
  const [dragStartHeight, setDragStartHeight] = useState(50);
  const minimizedHeight = 10;
  const { t } = useTranslation();

  // The collapsed panel uses content-driven height, which can be taller than a
  // nominal vh value in short windows. Report the real rendered height so the
  // transcript reserves exactly the space the panel occupies.
  useEffect(() => {
    const panel = panelRef.current;
    if (!panel || !show || typeof ResizeObserver === 'undefined') return;

    let frame: number | undefined;
    const reportHeight = () => {
      if (frame !== undefined) cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const viewportHeight = Math.max(window.innerHeight, 1);
        const heightInVh = (panel.getBoundingClientRect().height / viewportHeight) * 100;
        onHeightChange?.(heightInVh);
      });
    };

    const observer = new ResizeObserver(reportHeight);
    observer.observe(panel);
    window.addEventListener('resize', reportHeight);
    reportHeight();

    return () => {
      observer.disconnect();
      window.removeEventListener('resize', reportHeight);
      if (frame !== undefined) cancelAnimationFrame(frame);
    };
  }, [show, onHeightChange]);

  // Receive Ask MeetingCopilot drafts/submissions from the minimized recording widget and
  // coordinate focus once the desktop window is active.
  useEffect(() => {
    const ipc = (window as any).ipc;
    if (!ipc?.on) return;
    let clearPendingFocus: (() => void) | undefined;
    const cleanup = ipc.on('open-ask', (payload?: { draft?: string; submit?: boolean }) => {
      clearPendingFocus?.();
      const requestedDraft = typeof payload?.draft === 'string' ? payload.draft : '';
      const isDirectSubmission = !!requestedDraft && payload?.submit === true;
      if (isDirectSubmission) pillSubmitRef.current(requestedDraft);
      else if (requestedDraft) setInput(requestedDraft);
      const focusDeadline = performance.now() + 3500;
      let retryTimer: number | undefined;

      const clearFocusAttempt = () => {
        if (retryTimer !== undefined) window.clearTimeout(retryTimer);
        window.removeEventListener('focus', focusInput);
        document.removeEventListener('visibilitychange', focusInput);
      };
      const focusInput = () => {
        const textarea = textareaRef.current;
        textarea?.focus({ preventScroll: true });

        const focusConfirmed = isDirectSubmission
          ? document.hasFocus()
          : !!textarea && document.hasFocus() && document.activeElement === textarea;
        if (focusConfirmed) {
          window.requestAnimationFrame(() => {
            const focusedTextarea = textareaRef.current;
            if (!focusedTextarea || isDirectSubmission) return;
            focusedTextarea.focus({ preventScroll: true });
            const cursorPosition = focusedTextarea.value.length;
            focusedTextarea.setSelectionRange(cursorPosition, cursorPosition);
          });
          ipc.send('ask-input-focused', {});
          clearFocusAttempt();
          return;
        }

        if (performance.now() < focusDeadline) {
          if (retryTimer !== undefined) window.clearTimeout(retryTimer);
          retryTimer = window.setTimeout(focusInput, 100);
        } else {
          clearFocusAttempt();
        }
      };

      // A macOS Space switch can take longer than a normal window restore.
      // Retry until the document and textarea are genuinely focused.
      window.addEventListener('focus', focusInput);
      document.addEventListener('visibilitychange', focusInput);
      clearPendingFocus = clearFocusAttempt;
      focusInput();
    });
    return () => {
      clearPendingFocus?.();
      if (typeof cleanup === 'function') cleanup();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Add custom CSS for text selection
  useEffect(() => {
    const style = document.createElement('style');
    style.textContent = `
      .chat-bubble ::selection {
        background-color: #4A9EFF !important;
        color: white !important;
      }
      .chat-bubble ::-moz-selection {
        background-color: #4A9EFF !important;
        color: white !important;
      }
      .chat-bubble * {
        user-select: text !important;
        -webkit-user-select: text !important;
        -moz-user-select: text !important;
        -ms-user-select: text !important;
      }
    `;
    document.head.appendChild(style);

    return () => {
      document.head.removeChild(style);
    };
  }, []);

  // Notify parent component of initial open state based on Real-time Summary toggle
  useEffect(() => {
    if (isToggleOn && open) {
      // Real-time Summary is ON and panel is open - notify parent
      onOpenChange?.(true);
      onHeightChange?.(dynamicHeight);
    } else if (!open) {
      // Ensure closed state has consistent height
      onHeightChange?.(minimizedHeight);
    }
  }, []); // Run only once on mount

  // Monitor Real-time Summary toggle changes during active session
  useEffect(() => {
    if (!isToggleOn && open) {
      // Real-time Summary turned OFF - minimize Ask MeetingCopilot panel
      productionLogger.debug('🔽 Real-time Summary turned OFF - minimizing Ask MeetingCopilot');
      setOpen(false);
      onOpenChange?.(false);
      onHeightChange?.(minimizedHeight); // Use consistent minimized height
    }
  }, [isToggleOn]); // Run when Real-time Summary toggle changes

  // Removed text selection tracking since we removed focus stealing

  // Mode states for button highlighting and placeholder
  const [activeMode, setActiveMode] = useState<'ask' | 'speech' | 'note'>('ask');
  const [copiedSpeech, setCopiedSpeech] = useState<string | null>(null);
  const [isComposing, setIsComposing] = useState(false);

  // Loading state for AI explanations
  const [loadingInsights, setLoadingInsights] = useState<{ [key: string]: boolean }>({});
  // UI state for showing explanations under summaries
  const [summaryInsights, setSummaryInsights] = useState<{ [key: string]: string }>({});

  // Scroll state tracking
  const [isUserScrolling, setIsUserScrolling] = useState(false);
  const [isAtBottom, setIsAtBottom] = useState(true);

  const [fontSize, setFontSize] = useState(() => {
    const savedFontSize = localStorage.getItem('textSize');
    return savedFontSize ? parseInt(savedFontSize) : 50; // Default 50%
  });

  useEffect(() => {
    const handleStorageChange = () => {
      const savedFontSize = localStorage.getItem('textSize');
      setFontSize(savedFontSize ? parseInt(savedFontSize) : 50);
    };

    window.addEventListener('storage', handleStorageChange);

    // Also listen for manual updates within the same tab
    const interval = setInterval(() => {
      const savedFontSize = localStorage.getItem('textSize');
      const currentSize = savedFontSize ? parseInt(savedFontSize) : 50;
      if (currentSize !== fontSize) {
        setFontSize(currentSize);
      }
    }, 500);

    return () => {
      window.removeEventListener('storage', handleStorageChange);
      clearInterval(interval);
    };
  }, [fontSize]);

  // Helper to split bundled content (for legacy support)
  const splitSummaryContent = (content: string | undefined) => {
    const c = (content || '').trim();
    if (!c) return { base: '', expl: '' };
    // Prefer explicit label format: "Summary\nExplanation: ..."
    const labelIdx = c.indexOf('\nExplanation:');
    if (labelIdx !== -1) {
      const base = c.slice(0, labelIdx).trim();
      const expl = c.slice(labelIdx + '\nExplanation:'.length).trim();
      return { base, expl };
    }
    // Backward compat: hyphen format "Summary - Explanation"
    const hyphenIdx = c.indexOf(' - ');
    if (hyphenIdx !== -1) {
      const base = c.slice(0, hyphenIdx).trim();
      const expl = c.slice(hyphenIdx + 3).trim();
      return { base, expl };
    }
    return { base: c, expl: '' };
  };

  // Handle scroll events to detect user scrolling
  useEffect(() => {
    const container = messagesContainerRef.current;
    if (!container) return;

    let scrollTimeout: ReturnType<typeof setTimeout> | undefined;

    const handleScroll = () => {
      setIsUserScrolling(true);
      checkIfAtBottom();

      // Clear the previous timeout
      clearTimeout(scrollTimeout);

      // Set a new timeout to reset user scrolling flag
      scrollTimeout = setTimeout(() => {
        setIsUserScrolling(false);
      }, 1000); // Reset after 1 second of no scrolling
    };

    container.addEventListener('scroll', handleScroll);

    // Initial check
    checkIfAtBottom();

    return () => {
      container.removeEventListener('scroll', handleScroll);
      clearTimeout(scrollTimeout);
    };
  }, [open]); // Re-run when panel opens/closes

  // Auto-scroll when new messages are added (but only if user is at bottom)
  useEffect(() => {
    if (messages.length > 0) {
      conditionalScrollToBottom();
    }
  }, [messages.length]);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  // Check if user is at the bottom of the chat
  const checkIfAtBottom = () => {
    const container = messagesContainerRef.current;
    if (!container) return false;

    const threshold = 50; // pixels from bottom
    const isAtBottom = container.scrollHeight - container.scrollTop - container.clientHeight <= threshold;
    setIsAtBottom(isAtBottom);
    return isAtBottom;
  };

  // Auto scroll only if user is at bottom
  const conditionalScrollToBottom = () => {
    if (isAtBottom && !isUserScrolling) {
      setTimeout(() => {
        messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
      }, 100);
    }
  };

  // Get placeholder text based on active mode
  const getPlaceholderText = () => {
    switch (activeMode) {
      case 'ask':
        return t('bullet.placeholder.ask');
      case 'speech':
        return t('bullet.placeholder.speech');
      case 'note':
        return t('bullet.placeholder.note');
      default:
        return t('bullet.placeholder.ask');
    }
  };

  // Handle mode toggle
  const handleModeToggle = (mode: 'ask' | 'speech' | 'note') => {
    setActiveMode(mode);
    setInput('');
    // Focus the textarea when mode is changed
    setTimeout(() => {
      textareaRef.current?.focus();
    }, 100);
  };

  // Copy speech text function
  const handleCopySpeech = async (speechText: string, messageIndex: number) => {
    try {
      await navigator.clipboard.writeText(speechText);
      setCopiedSpeech(`${messageIndex}`);
      setTimeout(() => setCopiedSpeech(null), 2000); // Reset after 2 seconds
    } catch (err) {
      productionLogger.error('Failed to copy text: ', err);
    }
  };

  // Handle summary insights click
  const handleSummaryClick = async (message: Message, messageIndex: number) => {
    productionLogger.debug('🖱️ Summary clicked:', { message, messageIndex, hasSummaryId: !!message.summaryId });

    // Generate fallback ID for existing summaries without summaryId
    let summaryId = message.summaryId;
    if (!summaryId) {
      summaryId = `fallback_${messageIndex}_${message.timestamp.getTime()}`;
      productionLogger.debug('⚠️ Using fallback summaryId:', summaryId);
      // Update the message to have this ID for future clicks
      setMessages(prev => prev.map((m, i) =>
        i === messageIndex ? { ...m, summaryId } : m
      ));
    }

    productionLogger.debug('📋 Current summaryInsights:', summaryInsights);

    // If insight already exists, expand/collapse it
    if (summaryInsights[summaryId]) {
      productionLogger.debug('🔄 Collapsing existing insight');
      setSummaryInsights(prev => {
        const newInsights = { ...prev };
        delete newInsights[summaryId!];
        return newInsights;
      });
      return;
    }


    // If already loading, don't make another request
    if (loadingInsights[summaryId]) {
      productionLogger.debug('⏳ Already loading insight for:', summaryId);
      return;
    }

    setLoadingInsights(prev => ({ ...prev, [summaryId!]: true }));

    try {
      // Get all summary messages for context
      const summaryMessages = messages.filter(msg => msg.isSummary);

      // Determine if this is a question (contains question mark)
      const isQuestion = message.content.includes('?');


      // Use the socketId we received from the server
      productionLogger.debug('📡 Using stored socket ID for explanation:', socketId);
      productionLogger.debug('📊 Transcript data length:', transcriptData?.length || 0);

      // Prefer the established socket.id if available, fallback to stored state
      const effectiveSessionId = socketId || socketRef.current?.id || null;
      if (!effectiveSessionId) {
        productionLogger.warn('⚠️ No socket sessionId available yet; server will attempt fallback matching');
      }

      // Call the backend to generate insights
      const response = await axios.post(`${baseURL}/chat/generate-summary-insight`, {
        summary: {
          text: message.content,
          timestamp: message.timestamp,
          speaker: message.speaker,
          summaryId: summaryId
        },
        allSummaries: summaryMessages.map(msg => ({
          text: msg.content,
          timestamp: msg.timestamp,
          speaker: msg.speaker,
          summaryId: msg.summaryId
        })),
        isQuestion,
        userId,
        translatedLanguage,
        transcriptData: transcriptData || [],
        sessionId: effectiveSessionId
      });

      if (response.data?.data?.insight) {
        // Add explanation as a new message for storage (simple like summaries)
        const explanationMessage = {
          role: 'assistant' as const,
          content: `> ${response.data.data.insight}`, // Add > prefix for storage
          timestamp: new Date(),
          isExplanation: true,
          summaryId: summaryId // Link to the summary
        };

        setMessages(prev => {
          const newMessages = [...prev, explanationMessage];
          return newMessages;
        });

        // Also update UI state to show explanation under summary
        setSummaryInsights(prev => ({
          ...prev,
          [summaryId!]: response.data.data.insight
        }));

        // Auto-scroll to bottom when AI explanation is created
        conditionalScrollToBottom();

      } else {
        productionLogger.warn('⚠️ No insight in response:', response.data);
        // Add error message as explanation message
        const errorMessage = {
          role: 'assistant' as const,
          content: `> ${t('insights.noInsightAvailable')}`, // Add > prefix for storage
          timestamp: new Date(),
          isExplanation: true,
          summaryId: summaryId
        };
        setMessages(prev => [...prev, errorMessage]);

        // Also show error in UI
        setSummaryInsights(prev => ({
          ...prev,
          [summaryId!]: t('insights.noInsightAvailable')
        }));
      }
    } catch (error: any) {
      productionLogger.error('❌ Error generating summary insight:', error);

      // Check if it's an AI credits limit error
      let errorText = t('insights.failedToGenerate');
      if (error.response?.status === 400 &&
        (error.response?.data?.message?.includes('AI credits limit exceeded') ||
          error.response?.data?.error?.includes('AI credits limit exceeded'))) {
        errorText = t('ask.outOfCredits');

        // Also trigger the AI credits limit exceeded UI (like in other parts of the app)
        setError('AI credits limit exceeded');
      }

      // Add error message as explanation message
      const errorMessage = {
        role: 'assistant' as const,
        content: `> ${errorText}`, // Add > prefix for storage
        timestamp: new Date(),
        isExplanation: true,
        summaryId: summaryId
      };
      setMessages(prev => [...prev, errorMessage]);

      // Also show error in UI
      setSummaryInsights(prev => ({
        ...prev,
        [summaryId!]: errorText
      }));
    } finally {
      setLoadingInsights(prev => ({ ...prev, [summaryId!]: false }));
    }
  };

  // Drag resize functionality
  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    setIsDragging(true);
    setDragStartY(e.clientY);
    setDragStartHeight(dynamicHeight);
  };

  const handleMouseMove = (e: MouseEvent) => {
    if (!isDragging) return;

    const deltaY = dragStartY - e.clientY; // Inverted because dragging up should increase height
    const newHeight = Math.max(20, Math.min(80, dragStartHeight + (deltaY / window.innerHeight) * 100));

    setDynamicHeight(newHeight);
    localStorage.setItem('askMeetingCopilotHeight', newHeight.toString());
    onHeightChange?.(newHeight);
  };

  const handleMouseUp = () => {
    setIsDragging(false);
  };

  // Add global mouse event listeners for dragging
  useEffect(() => {
    if (isDragging) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = 'ns-resize';
      document.body.style.userSelect = 'none';
    } else {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    }

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [isDragging, dragStartY, dragStartHeight, dynamicHeight]);

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  // Load explanations from stored explanation messages into UI state
  useEffect(() => {
    if (!messages || messages.length === 0) return;

    const newInsights: { [key: string]: string } = {};

    // Find explanation messages and map them to their related summaries using summaryId
    messages.forEach((message) => {
      if (message.isExplanation && message.summaryId) {
        if (!summaryInsights[message.summaryId]) {
          // Remove > prefix for UI display under summary (first explanation)
          const cleanContent = message.content?.startsWith('> ')
            ? message.content.slice(2)
            : message.content;
          newInsights[message.summaryId] = cleanContent;
        }
      }
    });

    // Also handle bundled content from loaded messages (legacy support)
    messages.forEach((m, i) => {
      if (!m?.isSummary || typeof m.content !== 'string' || !m.summaryId) return;
      const { base, expl } = splitSummaryContent(m.content);
      if (expl) {
        if (!summaryInsights[m.summaryId]) {
          newInsights[m.summaryId] = expl;
        }
        // Update message to only show base content
        if (base !== m.content) {
          setMessages(prev => prev.map((msg, idx) =>
            idx === i ? { ...msg, content: base } : msg
          ));
        }
      }
    });

    if (Object.keys(newInsights).length > 0) {
      setSummaryInsights(prev => ({ ...prev, ...newInsights }));
    }
  }, [messages.length]); // Only run when message count changes

  // Listen for custom focus events from parent component (tab switching)
  useEffect(() => {
    const handleFocusEvent = () => {
      // Focus the input field when requested
      setTimeout(() => {
        textareaRef.current?.focus();
      }, 50);
    };

    window.addEventListener('focusAskMeetingCopilot', handleFocusEvent);

    return () => {
      window.removeEventListener('focusAskMeetingCopilot', handleFocusEvent);
    };
  }, []);

  // Listen for socket connection and store socket ID
  useEffect(() => {
    const socket = socketRef.current;
    if (!socket) return;

    const handleSocketConnected = (data: { socketId: string; timestamp: string }) => {
      productionLogger.debug('📡 Received socket ID from server:', data.socketId);
      setSocketId(data.socketId);
    };

    socket.on('socket_connected', handleSocketConnected);

    return () => {
      socket.off('socket_connected', handleSocketConnected);
    };
  }, [socketRef]);

  // Listen for bullet explanation generated events from socket
  useEffect(() => {
    const socket = socketRef.current;
    if (!socket) return;

    const handleBulletExplanationGenerated = (data: { summaryId: string; explanation: string }) => {
      const { summaryId, explanation } = data;

      // Add explanation as a new message for storage
      const explanationMessage = {
        role: 'assistant' as const,
        content: `> ${explanation}`, // Add > prefix for storage
        timestamp: new Date(),
        isExplanation: true,
        summaryId: summaryId
      };
      setMessages(prev => [...prev, explanationMessage]);

      // Update UI state to show explanation under summary
      setSummaryInsights(prev => ({
        ...prev,
        [summaryId]: explanation.startsWith('> ') ? explanation.slice(2) : explanation
      }));

      // Auto-scroll to bottom when socket explanation is received
      conditionalScrollToBottom();

      productionLogger.debug('🧩 Socket explanation saved and shown in UI:', { summaryId, explanation });
    };

    socket.on('bullet_explanation_generated', handleBulletExplanationGenerated);

    return () => {
      socket.off('bullet_explanation_generated', handleBulletExplanationGenerated);
    };
  }, [socketRef, messages]);

  // Removed aggressive auto-focus to prioritize text selection and copying

  // Removed global keyboard capture and aggressive focus management to prioritize text selection

  // Remove all height manipulation - use pure CSS for stability

  // Dynamic height calculation based on content - only run once when opened
  useEffect(() => {
    let focusTimer: number | undefined;

    // Only change height when panel is first opened from closed state
    // Don't change height if panel was already open (tab switching)
    if (open && dynamicHeight === 50) {
      // Keep user's preferred height when maximized - only set once
      const savedHeight = localStorage.getItem('askMeetingCopilotHeight');
      const preferredHeight = savedHeight ? parseInt(savedHeight) : 50;

      // Only update if it's different from default to prevent unnecessary re-renders
      if (preferredHeight !== 50) {
        setDynamicHeight(preferredHeight);
        onHeightChange?.(preferredHeight);
      }
    }

    // Auto-focus when panel opens
    if (open && autoFocusInput) {
      focusTimer = window.setTimeout(() => {
        textareaRef.current?.focus();
      }, 300); // Slightly longer delay to ensure panel is fully opened
    }

    return () => {
      if (focusTimer !== undefined) window.clearTimeout(focusTimer);
    };
  }, [open, autoFocusInput]); // Removed prompt3, prompt4 dependencies to prevent size changes

  const handleTransitionEnd = () => {
    // Transition ended
  };

  const handleToggle = () => {
    const newOpen = !open;
    setOpen(newOpen);
    onOpenChange?.(newOpen);

    if (newOpen) {
      productionLogger.debug('🔼 Ask MeetingCopilot opening - height:', dynamicHeight);
      onHeightChange?.(dynamicHeight);
      setTimeout(conditionalScrollToBottom, 300);
    } else {
      productionLogger.debug('🔽 Ask MeetingCopilot closing - height:', minimizedHeight);
      // Use fixed minimized height to prevent size fluctuations
      onHeightChange?.(minimizedHeight);
    }
  };

  const handleSubmit = async (e?: React.FormEvent, inputOverride?: string, forceAsk = false) => {
    e?.preventDefault();
    const submittedInput = inputOverride ?? input;
    if (!submittedInput.trim() || isLoading) return;

    setError(null);

    // Expand Ask MeetingCopilot if not already open
    if (!open) {
      setOpen(true);
      onOpenChange?.(true);
      onHeightChange?.(dynamicHeight);
    }

    // Check if input starts with "Generate speech" (with or without colon for backward compatibility)
    const lowerInput = submittedInput.toLowerCase();
    if (!forceAsk && (activeMode === 'speech' || lowerInput.startsWith('generate speech: ') || lowerInput.startsWith('generate speech '))) {
      let userPrompt;
      if (activeMode === 'speech') {
        userPrompt = submittedInput;
      } else if (lowerInput.startsWith('generate speech: ')) {
        userPrompt = submittedInput.substring('generate speech: '.length);
      } else {
        userPrompt = submittedInput.substring('generate speech '.length);
      }
      if (userPrompt.trim()) {
        const userMessage = {
          role: 'user' as const,
          content: activeMode === 'speech' ? `Generate speech ${submittedInput}` : submittedInput,
          timestamp: new Date()
        };
        setMessages(prev => [...prev, userMessage]);
        setInput(''); // Clear input immediately after sending

        setIsLoading(true);
        setTimeout(conditionalScrollToBottom, 100);

        try {
          // Format transcript for context - last 2 speaker chunks
          const transcriptText = transcriptData && transcriptData.length > 0
            ? transcriptData.slice(-2).map(entry => {
              const text = entry.captions.map((c: any) => c.text).join(' ');
              const translation = entry.translatedText ? ` (${entry.translatedText})` : '';
              return `${entry.speakerName}: ${text}${translation}`;
            }).join('\n')
            : '';

          const response = await axios.post(`${baseURL}/chat/generate-speech-view`, {
            transcript: transcriptText,
            userId,
            userPrompt: userPrompt.trim(),
            spokenLanguage: spokenLanguage
          });

          if (response.data?.data) {
            const { speechText, pronunciation } = response.data.data;

            const assistantMessage = {
              role: 'assistant' as const,
              content: `Speech generated in ${spokenLanguage}`,
              timestamp: new Date(),
              speechData: {
                speechText,
                pronunciation,
                language: spokenLanguage
              }
            };
            setMessages(prev => [...prev, assistantMessage]);
          }
        } catch (error: any) {
          productionLogger.error('Error generating speech view:', error);
          const errorMessage = {
            role: 'assistant' as const,
            content: t('speechGeneration.failedToGenerate'),
            timestamp: new Date()
          };
          setMessages(prev => [...prev, errorMessage]);
        } finally {
          setIsLoading(false);
        }
      }
      return;
    }

    // Check if input starts with "Take a note" (with or without colon for backward compatibility)
    if (!forceAsk && (activeMode === 'note' || lowerInput.startsWith('take a note: ') || lowerInput.startsWith('take a note '))) {
      let noteContent;
      if (activeMode === 'note') {
        noteContent = submittedInput;
      } else if (lowerInput.startsWith('take a note: ')) {
        noteContent = submittedInput.substring('take a note: '.length);
      } else {
        noteContent = submittedInput.substring('take a note '.length);
      }
      if (noteContent) {
        // Add to notepad with HTML formatting (TipTap uses HTML)
        setNotepad(prev => {
          // If prev is empty or just <p></p>, start fresh
          if (!prev || prev === '<p></p>' || prev.trim() === '') {
            return `<p>${noteContent}</p>`;
          }
          // Remove closing </p> from prev if exists, add new content as new paragraph
          const trimmedPrev = prev.trim();
          if (trimmedPrev.endsWith('</p>')) {
            return `${trimmedPrev}<p>${noteContent}</p>`;
          }
          return `${trimmedPrev}<p>${noteContent}</p>`;
        });

        // Show confirmation message
        const userMessage = {
          role: 'user' as const,
          content: activeMode === 'note' ? `Quick note ${submittedInput}` : submittedInput,
          timestamp: new Date()
        };
        const confirmMessage = {
          role: 'assistant' as const,
          content: t('bullet.memoSaved'),
          timestamp: new Date(),
          isMemoConfirmation: true
        };
        setMessages(prev => [...prev, userMessage, confirmMessage]);
      }
      setInput('');
      setTimeout(conditionalScrollToBottom, 100);
      return;
    }

    const userMessage = {
      role: 'user' as const,
      content: submittedInput,
      timestamp: new Date()
    };
    setMessages(prev => [...prev, userMessage]);
    setInput(''); // Clear input immediately after sending

    setIsLoading(true);
    // Force scroll for user messages since they initiated the action
    setTimeout(scrollToBottom, 100);

    try {
      const response = await axios.post(`${baseURL}/chat/ask`, {
        question: submittedInput,
        userId,
        transcriptData,
        chatHistory: messages
      });

      if (response.data?.data.error) {
        setError(response.data?.data.error);
        if (response.data?.data.error === 'AI credits limit exceeded') {
          if (response.data?.data.creditUsage) {
            setCreditUsage(response.data?.data.creditUsage);
          }
        }
      } else {
        const assistantMessage = {
          role: 'assistant' as const,
          content: response.data.data.answer,
          timestamp: new Date()
        };
        setMessages(prev => [...prev, assistantMessage]);

        if (response.data.data.creditUsage) {
          setCreditUsage(response.data.data.creditUsage);
        }
      }
    } catch (error: any) {
      productionLogger.error('Error sending question:', error.response?.data?.message);
      if (error.response?.data?.message?.includes('AI credits limit exceeded')) {
        setError('AI credits limit exceeded');
      }
    } finally {
      setIsLoading(false);
    }
  };

  pillSubmitRef.current = (question: string) => {
    setActiveMode('ask');
    void handleSubmit(undefined, question, true);
  };

  if (!show) return null;

  return (
    <>
      <div
        ref={panelRef}
        className="ask-MeetingCopilot-panel fixed left-0 right-0 bottom-0 bg-[#1F1F1F] z-50 flex flex-col pointer-events-auto transition-all duration-300 border-t border-[#333] rounded-b-xl"
        style={{
          left: 'var(--live-sidebar-offset, 0px)',
          height: open ? (isWindows ? `${dynamicHeight}vh` : `${dynamicHeight}dvh`) : (isWindows ? 'auto' : 'auto'),
          minHeight: 'auto',
          borderTopLeftRadius: 16,
          borderTopRightRadius: 16,
          boxShadow: '0 -6px 24px rgba(0, 0, 0, 0.4)',
        }}
        onTransitionEnd={handleTransitionEnd}
      >
        {/* Invisible resize handle - only functional when maximized */}
        {open && (
          <div
            ref={resizeHandleRef}
            onMouseDown={handleMouseDown}
            className="absolute -top-1 left-0 right-0 h-2 cursor-ns-resize"
            style={{ borderTopLeftRadius: 16, borderTopRightRadius: 16 }}
          />
        )}
        <div
          className={`ask-MeetingCopilot-panel__header bg-[#1F1F1F] flex items-center justify-between px-4 select-none hover:bg-[#252525] transition-all duration-200 ${open ? 'pt-2 pb-0.5' : 'py-1.5'}`}
          style={{
            borderTopLeftRadius: 16,
            borderTopRightRadius: 16,
          }}
        >
          {/* Left side: Icon + Ask MeetingCopilot text */}
          <div className="flex items-center gap-2 cursor-pointer" onClick={handleToggle}>
            <div className="flex items-center">
              {open ? (
                <svg className="text-gray-300 hover:text-white transition-colors w-5 h-5" viewBox="0 0 24 24"><path fill="currentColor" d="M7.41 15.41L12 10.83l4.59 4.58L18 14l-6-6-6 6z" /></svg>
              ) : (
                <svg className="text-gray-300 hover:text-white transition-colors w-5 h-5" viewBox="0 0 24 24"><path fill="currentColor" d="M7.41 8.59L12 13.17l4.59-4.58L18 10l-6 6-6-6z" /></svg>
              )}
            </div>
            <span className="text-white font-medium text-base">{t('bullet.title')}</span>
          </div>

          {/* Right side: Real-time Summary toggle */}
          <div className="flex items-center gap-1.5">
            <span className={`text-xs text-white transition-opacity ${isToggleOn ? 'opacity-100' : 'opacity-50'}`}>{t('realTimeSummary')}</span>
            <button
              onClick={(e) => {
                e.stopPropagation(); // Prevent triggering the panel toggle
                const newToggleState = !isToggleOn;
                setIsToggleOn(newToggleState);

                // Auto-maximize when Real-time Summary is turned ON
                if (newToggleState && !open) {
                  setOpen(true);
                  onOpenChange?.(true);
                  onHeightChange?.(dynamicHeight);
                }
              }}
              className={`relative inline-flex h-4 w-8 items-center rounded-full transition-all focus:outline-none focus:ring-1 focus:ring-offset-1 focus:ring-[#484dff] border ${isToggleOn
                  ? 'bg-gradient-to-br from-[#383bff] via-[#484dff] to-[#585dff] border-[#484dff] opacity-100'
                  : 'bg-gray-700 border-gray-500 opacity-50'
                }`}
            >
              <span
                className={`inline-block h-3 w-3 transform rounded-full bg-white transition-transform shadow-lg border-2 border-gray-200 ring-1 ring-gray-400 ${isToggleOn ? 'translate-x-4' : 'translate-x-0.5'
                  }`}
              />
            </button>
          </div>
        </div>

        {/* Section 2: Chat Bubbles - Only show when open */}
        {open && (
          <div className="ask-MeetingCopilot-panel__body flex-1 bg-[#1F1F1F] text-white overflow-hidden">
            {error && error.includes('AI credits limit exceeded') && (
              <div className="absolute inset-0 bg-[#1F1F1F]/95 flex flex-col items-center justify-center backdrop-blur-sm z-20 rounded-b-[24px]">
                <div className="bg-gradient-to-br from-[#1A1A1A] via-[#2A2A2A] to-[#333] rounded-3xl p-6 mx-4 border-2 border-[#404040] shadow-2xl max-w-sm">
                  <h2 className="text-lg font-bold text-white mb-3 text-center">
                    {t("ask.creditLimit")}
                  </h2>
                  <p className="text-gray-300 text-center mb-4 leading-relaxed text-sm">
                    <span className="block mb-2">
                      {t("ask.creditLimitMessage1")} <span className="font-semibold text-white">{t("ask.creditLimitMessage2")}</span>
                    </span>
                  </p>

                  {creditUsage && (
                    <div className="bg-gradient-to-br from-[#111] to-[#1A1A1A] rounded-2xl p-4 border-2 border-[#333] shadow-lg mb-4">
                      <div className="text-center">
                        <p className="text-gray-300 text-sm mb-3">
                          {t("ask.usedCredits")}: <span className="font-bold text-white">{creditUsage.used}/{creditUsage.limit}</span>
                        </p>
                        <div className="w-full bg-[#333] rounded-full h-2.5 overflow-hidden">
                          <div
                            className="bg-gradient-to-r from-blue-500 to-purple-600 h-full rounded-full transition-all duration-500"
                            style={{ width: `${Math.min((creditUsage.used / creditUsage.limit) * 100, 100)}%` }}
                          ></div>
                        </div>
                      </div>
                    </div>
                  )}

                  {/* Upgrade Now Button */}
                  <div className="text-center">
                    <UpgradeNow
                      referrer="desktop-ai-credits"
                      className="bg-gradient-to-r from-[#383bff] via-[#484dff] to-[#585dff] hover:from-[#282bff] hover:to-[#383bff] text-white font-semibold py-3 px-6 rounded-full transition-all duration-200 shadow-lg hover:shadow-xl border border-[#484dff] hover:border-[#383bff] text-sm"
                      text={t("upgrade.title")}
                    />
                  </div>
                </div>
              </div>
            )}

            <div
              ref={messagesContainerRef}
              className={`h-full overflow-y-auto scrollbar-thin scrollbar-thumb-gray-600 scrollbar-track-transparent ${error && error.includes('AI credits limit exceeded') ? 'blur-sm' : ''}`}
              style={{ paddingBottom: `10px` }}
            >
              {messages.filter(message => !message.isExplanation).map((message, index) => (
                <div key={index}>
                  {/* Small label for message type */}
                  {message.role === 'user' && (message.content.startsWith('Generate speech ') || message.content.startsWith('Quick note ')) && (
                    <div className={`flex ${message.role === 'user' ? 'justify-end' : 'justify-start'} px-4 pt-1`}>
                      <span className="text-[10px] text-gray-500 opacity-50 font-medium">
                        {message.content.startsWith('Generate speech ') ? t('bullet.generateSpeech') : t('bullet.quickNote')}
                      </span>
                    </div>
                  )}
                  <div className={`flex ${message.role === 'user' ? 'justify-end' : 'justify-start'} px-4 ${message.role === 'user' && (message.content.startsWith('Generate speech ') || message.content.startsWith('Quick note ')) ? 'pt-0' : 'pt-2'} pb-1`}>
                    <div
                      className={`chat-bubble ${message.role === 'user' ? 'max-w-[80%]' : 'max-w-[95%]'} px-4 py-1 shadow-xl transition-all duration-200 border-2 flex items-start select-text ${message.role === 'user'
                        ? 'bg-gradient-to-br from-[#383bff] via-[#484dff] to-[#585dff] text-white border-[#484dff] hover:from-[#282bff] hover:to-[#383bff]'
                        : 'bg-gradient-to-br from-[#1A1A1A] via-[#2A2A2A] to-[#333] text-gray-100 border-[#404040] hover:from-[#222] hover:to-[#444]'
                        }`}
                      style={{
                        borderRadius: '20px',
                        userSelect: 'text',
                        WebkitUserSelect: 'text',
                        MozUserSelect: 'text',
                        msUserSelect: 'text'
                      }}
                    >
                      {/* Special UI for Generate Speech responses */}
                      {message.speechData ? (
                        <div className="text-left w-full">
                          {/* Speech Section */}
                          <div className="mb-3">
                            <div className="flex items-center justify-between mb-2">
                              <div className="flex items-center gap-2">
                                <svg className="w-4 h-4 text-[#484dff]" fill="currentColor" viewBox="0 0 24 24">
                                  <path d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
                                </svg>
                                <h3 className="text-[#484dff] font-semibold text-sm">{t('bullet.speechIn')} {message.speechData.language}</h3>
                              </div>
                              <button
                                onClick={() => handleCopySpeech(message.speechData.speechText, index)}
                                className="flex items-center gap-1 px-2 py-1 text-xs text-white hover:bg-white/10 rounded-md transition-colors duration-200"
                              >
                                {copiedSpeech === `${index}` ? (
                                  <>
                                    <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 24 24">
                                      <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" />
                                    </svg>
                                    {t('bullet.copied')}
                                  </>
                                ) : (
                                  <>
                                    <svg className="w-3 h-3" fill="currentColor" viewBox="0 0 24 24">
                                      <path d="M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2zm0 16H8V7h11v14z" />
                                    </svg>
                                    {t('bullet.copy')}
                                  </>
                                )}
                              </button>
                            </div>
                            <div className="bg-gradient-to-br from-[#2A2A2A] to-[#1A1A1A] rounded-xl p-3 border border-[#404040]">
                              <p
                                className="text-white leading-relaxed"
                                style={{ fontSize: `${Math.max(12, Math.round(16 * (fontSize / 50)))}px` }}
                              >
                                {message.speechData.speechText}
                              </p>
                            </div>
                          </div>

                          {/* Pronunciation Section */}
                          <div>
                            <div className="flex items-center gap-2 mb-2">
                              <svg className="w-4 h-4 text-[#6b6bff]" fill="currentColor" viewBox="0 0 24 24">
                                <path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z" />
                              </svg>
                              <h3 className="text-[#6b6bff] font-semibold text-sm">{t('bullet.pronunciation')}</h3>
                            </div>
                            <div className="bg-gradient-to-br from-[#1F1F2A] to-[#1A1A1A] rounded-xl p-3 border border-[#484dff]/30">
                              <p
                                className="text-[#b8b8ff] leading-relaxed font-mono"
                                style={{ fontSize: `${Math.max(11, Math.round(14 * (fontSize / 50)))}px` }}
                              >
                                {message.speechData.pronunciation}
                              </p>
                            </div>
                          </div>
                        </div>
                      ) : message.isSummary ? (
                        /* Real-time Summary with point symbol */
                        <div className="w-full relative group">
                          <div className="flex items-start gap-2 text-left cursor-pointer hover:bg-white/5 rounded-lg p-1 transition-all duration-200"
                            onClick={() => handleSummaryClick(message, index)}>
                            <div className="flex-shrink-0 mt-2">
                              <div className="w-2 h-2 bg-gradient-to-br from-[#383bff] via-[#484dff] to-[#585dff] rounded-full"></div>
                            </div>
                            <div className="flex-1">
                              <div className="text-gray-100 leading-relaxed" style={{ fontSize: `${Math.max(10, Math.round(14 * (fontSize / 50)))}px` }}>
                                {message.content}
                              </div>
                            </div>
                          </div>
                          {/* Tooltip - only show if no explanation exists AND not currently loading */}
                          {(() => {
                            const summaryId = message.summaryId || `fallback_${index}_${message.timestamp.getTime()}`;
                            const hasInsight = summaryInsights[summaryId];
                            const isLoading = loadingInsights[summaryId];

                            if (!hasInsight && !isLoading) {
                              return (
                                <div className="absolute -bottom-8 right-0 opacity-0 group-hover:opacity-60 transition-opacity duration-500 pointer-events-none z-10">
                                  <div className="bg-gray-800/90 text-gray-300 text-xs px-2 py-1 rounded whitespace-nowrap border border-gray-700/50 shadow-lg">
                                    {t('bullet.aiExplanation')}
                                  </div>
                                </div>
                              );
                            }
                            return null;
                          })()}
                          {(() => {
                            const summaryId = message.summaryId || `fallback_${index}_${message.timestamp.getTime()}`;
                            const insight = summaryInsights[summaryId];
                            const isLoading = loadingInsights[summaryId];

                            if (isLoading) {
                              return (
                                <div className="ml-5 mt-2 p-3 bg-gradient-to-br from-[#2A2A2A] to-[#1A1A1A] rounded-lg border border-[#404040] animate-pulse">
                                  <div className="flex items-center gap-2">
                                    <div className="animate-spin rounded-full h-3 w-3 border-2 border-gray-300 border-t-[#484dff]"></div>
                                    <span className="text-xs text-gray-300">{t('bullet.analyzingContext')}</span>
                                  </div>
                                </div>
                              );
                            } else if (insight) {
                              return (
                                <div className="ml-5 mt-2 p-3 bg-gradient-to-br from-[#2A2A2A] to-[#1A1A1A] rounded-lg border border-[#404040] animate-slide-in">
                                  <div className="text-sm text-gray-200 leading-relaxed">
                                    {insight}
                                  </div>
                                </div>
                              );
                            }
                            return null;
                          })()}
                        </div>
                      ) : message.isMemoConfirmation ? (
                        /* Memo confirmation with View memo button */
                        <div className="flex items-center gap-3 text-left">
                          <span className="text-gray-100" style={{ fontSize: `${Math.max(10, Math.round(14 * (fontSize / 50)))}px` }}>
                            {message.content}
                          </span>
                          {onNavigateToMemos && (
                            <button
                              onClick={onNavigateToMemos}
                              className="px-3 py-1 text-xs bg-gradient-to-br from-[#383bff] via-[#484dff] to-[#585dff] text-white rounded-full hover:from-[#282bff] hover:to-[#383bff] transition-all duration-200 border border-[#484dff] whitespace-nowrap"
                            >
                              {t('bullet.viewMemo')}
                            </button>
                          )}
                        </div>
                      ) : (
                        /* Regular message content */
                        <div className="text-left prose prose-invert max-w-none mt-1 px-1 select-text
                      prose-li:marker:text-gray-400
                      prose-li:my-0.5
                      prose-li:ml-0
                      prose-li:pl-0
                      prose-ul:my-1
                      prose-ul:ml-0
                      prose-ul:pl-4
                      prose-ol:my-1
                      prose-ol:ml-0
                      prose-ol:pl-4
                      prose-h1:text-lg
                      prose-h1:font-semibold
                      prose-h1:text-white
                      prose-h1:mt-1
                      prose-h1:mb-1
                      prose-h1:ml-0
                      prose-h2:text-base
                      prose-h2:font-medium
                      prose-h2:text-white
                      prose-h2:mt-1
                      prose-h2:mb-0.5
                      prose-h2:ml-0
                      prose-h3:text-sm
                      prose-h3:font-medium
                      prose-h3:text-white
                      prose-h3:mt-0.5
                      prose-h3:mb-0.5
                      prose-h3:ml-0
                      prose-p:text-gray-100
                      prose-p:leading-relaxed
                      prose-p:my-1
                      prose-p:ml-0
                      prose-strong:text-white
                      prose-strong:font-semibold
                      prose-em:text-gray-200
                      prose-code:text-gray-200
                      prose-code:bg-gray-700
                      prose-code:px-1.5
                      prose-code:py-0.5
                      prose-code:rounded
                      prose-pre:bg-gray-800
                      prose-pre:text-gray-100
                      prose-pre:ml-0
                      prose-blockquote:border-l-gray-500
                      prose-blockquote:text-gray-300
                      prose-blockquote:ml-0
                      prose-a:text-blue-400
                      prose-a:no-underline
                      hover:prose-a:underline"
                          style={{
                            fontSize: '14px',
                            lineHeight: '1.4',
                            textAlign: 'left',
                            marginLeft: '0',
                            paddingLeft: '0',
                            userSelect: 'text',
                            WebkitUserSelect: 'text',
                            MozUserSelect: 'text',
                            msUserSelect: 'text'
                          }}
                        >
                          <ReactMarkdown
                            components={{
                              h1: ({ children }) => <h1 style={{ fontSize: `${Math.max(10, Math.round(15 * (fontSize / 50)))}px`, marginTop: '8px', marginBottom: '4px' }}>{children}</h1>,
                              h2: ({ children }) => <h2 style={{ fontSize: `${Math.max(10, Math.round(13 * (fontSize / 50)))}px`, marginTop: '6px', marginBottom: '3px' }}>{children}</h2>,
                              h3: ({ children }) => <h3 style={{ fontSize: `${Math.max(10, Math.round(12 * (fontSize / 50)))}px`, marginTop: '4px', marginBottom: '2px' }}>{children}</h3>,
                              p: ({ children }) => <p style={{ fontSize: `${Math.max(10, Math.round(14 * (fontSize / 50)))}px`, marginBottom: '4px', lineHeight: '1.2' }}>{children}</p>,
                              ul: ({ children }) => <ul style={{ fontSize: `${Math.max(10, Math.round(14 * (fontSize / 50)))}px`, marginBottom: '4px', paddingLeft: '12px', listStyleType: 'disc', listStylePosition: 'outside' }}>{children}</ul>,
                              ol: ({ children }) => <ol style={{ fontSize: `${Math.max(10, Math.round(14 * (fontSize / 50)))}px`, marginBottom: '4px', paddingLeft: '12px', listStyleType: 'decimal', listStylePosition: 'outside' }}>{children}</ol>,
                              li: ({ children }) => <li style={{ fontSize: `${Math.max(10, Math.round(14 * (fontSize / 50)))}px`, marginBottom: '0px', display: 'list-item' }}>{children}</li>,
                              a: ({ href, children }) => (
                                <span
                                  onClick={() => {
                                    if (href) {
                                      (window as any).electronAPI?.openExternal(href);
                                    }
                                  }}
                                  style={{
                                    color: '#4A9EFF',
                                    textDecoration: 'underline',
                                    cursor: 'pointer',
                                    fontSize: `${Math.max(10, Math.round(14 * (fontSize / 50)))}px`
                                  }}
                                  onMouseEnter={(e) => (e.target as HTMLElement).style.color = '#6BB3FF'}
                                  onMouseLeave={(e) => (e.target as HTMLElement).style.color = '#4A9EFF'}
                                >
                                  {children}
                                </span>
                              ),
                            }}
                          >
                            {message.content.startsWith('Generate speech ') ? message.content.substring('Generate speech '.length) :
                              message.content.startsWith('Quick note ') ? message.content.substring('Quick note '.length) :
                                message.content}
                          </ReactMarkdown>
                        </div>
                      )}
                    </div>
                  </div>

                </div>
              ))}
              {isLoading && (
                <div className="flex justify-start px-4 pt-2 pb-1">
                  <div className="max-w-[80%] px-4 py-2 shadow-xl transition-all duration-200 border-2 bg-gradient-to-br from-[#1A1A1A] via-[#2A2A2A] to-[#333] text-gray-100 border-[#404040] hover:from-[#222] hover:to-[#444]" style={{ borderRadius: '20px' }}>
                    <ThinkingAnimation />
                  </div>
                </div>
              )}
              <div ref={messagesEndRef} />
            </div>
          </div>
        )}

        {/* Section 3: Prompts + Chatbox - Always visible */}
        <div className={`ask-MeetingCopilot-panel__composer bg-gradient-to-t from-[#1F1F1F] via-[#1F1F1F] to-[#1F1F1F]/90 backdrop-blur-sm ${open ? 'pb-3' : 'pb-2'} ${!open ? ((prompt3 || prompt4) ? 'pt-1.5' : 'pt-1') : ((prompt3 || prompt4) ? 'pt-2' : 'pt-3')} px-3 ${error && error.includes('AI credits limit exceeded') ? 'blur-sm' : ''} ${!open ? 'flex-1' : ''}`}>
          <div ref={promptButtonsRef} className={`flex flex-wrap gap-1.5 ${(prompt3 || prompt4) ? 'mb-2' : 'hidden'}`}>
            {prompt3 && (
              <button
                onClick={async (e) => {
                  setInput(prompt3);

                  if (!open) {
                    setOpen(true);
                    onOpenChange?.(true);
                    onHeightChange?.(dynamicHeight);
                  }

                  setTimeout(() => {
                    textareaRef.current?.focus();
                  }, 100);

                  setTimeout(async () => {
                    const userMessage = {
                      role: 'user' as const,
                      content: prompt3,
                      timestamp: new Date()
                    };
                    setMessages(prev => [...prev, userMessage]);

                    setIsLoading(true);
                    setTimeout(conditionalScrollToBottom, 100);

                    try {
                      const response = await axios.post(`${baseURL}/chat/ask`, {
                        question: prompt3,
                        userId,
                        transcriptData,
                        chatHistory: messages
                      });

                      if (response.data?.data.error) {
                        setError(response.data?.data.error);
                        if (response.data?.data.error === 'AI credits limit exceeded') {
                          if (response.data?.data.creditUsage) {
                            setCreditUsage(response.data?.data.creditUsage);
                          }
                        }
                      } else {
                        const assistantMessage = {
                          role: 'assistant' as const,
                          content: response.data.data.answer,
                          timestamp: new Date()
                        };
                        setMessages(prev => [...prev, assistantMessage]);

                        if (response.data.data.creditUsage) {
                          setCreditUsage(response.data.data.creditUsage);
                        }
                      }
                    } catch (error: any) {
                      productionLogger.error('Error sending question:', error.response?.data?.message);
                      if (error.response.data?.message.includes('AI credits limit exceeded')) {
                        setError('AI credits limit exceeded');
                      }
                    } finally {
                      setIsLoading(false);
                    }
                  }, 500);
                }}
                className="px-3 py-1.5 bg-gradient-to-br from-[#333] via-[#555] to-[#777] text-white text-sm hover:from-[#444] hover:to-[#666] transition-all duration-200 whitespace-nowrap border-2 border-[#404040] shadow-lg hover:shadow-xl focus:outline-none focus:ring-2 focus:ring-[#484dff]/50 focus:border-[#484dff]"
                style={{ borderRadius: '50px' }}
              >
                {prompt3}
              </button>
            )}
            {prompt4 && (
              <button
                onClick={async (e) => {
                  setInput(prompt4);

                  if (!open) {
                    setOpen(true);
                    onOpenChange?.(true);
                    onHeightChange?.(dynamicHeight);
                  }

                  setTimeout(() => {
                    textareaRef.current?.focus();
                  }, 100);

                  setTimeout(async () => {
                    const userMessage = {
                      role: 'user' as const,
                      content: prompt4,
                      timestamp: new Date()
                    };
                    setMessages(prev => [...prev, userMessage]);

                    setIsLoading(true);
                    setTimeout(conditionalScrollToBottom, 100);

                    try {
                      const response = await axios.post(`${baseURL}/chat/ask`, {
                        question: prompt4,
                        userId,
                        transcriptData,
                        chatHistory: messages
                      });

                      if (response.data?.data.error) {
                        setError(response.data?.data.error);
                        if (response.data?.data.error === 'AI credits limit exceeded') {
                          if (response.data?.data.creditUsage) {
                            setCreditUsage(response.data?.data.creditUsage);
                          }
                        }
                      } else {
                        const assistantMessage = {
                          role: 'assistant' as const,
                          content: response.data.data.answer,
                          timestamp: new Date()
                        };
                        setMessages(prev => [...prev, assistantMessage]);

                        if (response.data.data.creditUsage) {
                          setCreditUsage(response.data.data.creditUsage);
                        }
                      }
                    } catch (error: any) {
                      productionLogger.error('Error sending question:', error.response?.data?.message);
                      if (error.response.data?.message.includes('AI credits limit exceeded')) {
                        setError('AI credits limit exceeded');
                      }
                    } finally {
                      setIsLoading(false);
                    }
                  }, 500);
                }}
                className="px-3 py-1.5 bg-gradient-to-br from-[#333] via-[#555] to-[#777] text-white text-sm hover:from-[#444] hover:to-[#666] transition-all duration-200 whitespace-nowrap border-2 border-[#404040] shadow-lg hover:shadow-xl focus:outline-none focus:ring-2 focus:ring-[#484dff]/50 focus:border-[#484dff]"
                style={{ borderRadius: '50px' }}
              >
                {prompt4}
              </button>
            )}
            {/*
            <button
              onClick={() => handleModeToggle('ask')}
              className={`px-3 py-1.5 text-sm hover:shadow-xl transition-all duration-200 whitespace-nowrap border-2 shadow-lg flex items-center focus:outline-none focus:ring-2 focus:ring-[#484dff]/50 ${activeMode === 'ask'
                  ? 'bg-gradient-to-br from-[#383bff] via-[#484dff] to-[#585dff] text-white border-[#484dff] hover:from-[#282bff] hover:to-[#383bff]'
                  : 'bg-gradient-to-br from-[#222] via-[#333] to-[#555] text-white hover:from-[#111] hover:to-[#444] border-[#404040] focus:border-[#484dff]'
                }`}
              style={{ borderRadius: '50px' }}
            >
              {t('bullet.ask')}
            </button>
            <button
              onClick={() => handleModeToggle('speech')}
              className={`px-3 py-1.5 text-sm hover:shadow-xl transition-all duration-200 whitespace-nowrap border-2 shadow-lg flex items-center gap-2 focus:outline-none focus:ring-2 focus:ring-[#484dff]/50 ${activeMode === 'speech'
                  ? 'bg-gradient-to-br from-[#383bff] via-[#484dff] to-[#585dff] text-white border-[#484dff] hover:from-[#282bff] hover:to-[#383bff]'
                  : 'bg-gradient-to-br from-[#222] via-[#333] to-[#555] text-white hover:from-[#111] hover:to-[#444] border-[#404040] focus:border-[#484dff]'
                }`}
              style={{ borderRadius: '50px' }}
            >
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
              </svg>
              {t('bullet.generateSpeech')}
            </button>
            <button
              onClick={() => handleModeToggle('note')}
              className={`px-3 py-1.5 text-sm hover:shadow-xl transition-all duration-200 whitespace-nowrap border-2 shadow-lg flex items-center gap-2 focus:outline-none focus:ring-2 focus:ring-[#484dff]/50 ${activeMode === 'note'
                  ? 'bg-gradient-to-br from-[#383bff] via-[#484dff] to-[#585dff] text-white border-[#484dff] hover:from-[#282bff] hover:to-[#383bff]'
                  : 'bg-gradient-to-br from-[#222] via-[#333] to-[#555] text-white hover:from-[#111] hover:to-[#444] border-[#404040] focus:border-[#484dff]'
                }`}
              style={{ borderRadius: '50px' }}
            >
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                <path d="M17.414 2.586a2 2 0 00-2.828 0L7 10.172V13h2.828l7.586-7.586a2 2 0 000-2.828z" />
                <path fillRule="evenodd" d="M2 6a2 2 0 012-2h4a1 1 0 010 2H4v10h10v-4a1 1 0 112 0v4a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
              </svg>
              {t('bullet.quickNote')}
            </button>
            */}
          </div>
          <form onSubmit={handleSubmit}>
            <div className="relative flex items-center">
              <textarea
                ref={textareaRef}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onCompositionStart={() => setIsComposing(true)}
                onCompositionEnd={() => setIsComposing(false)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey && !isComposing) {
                    e.preventDefault();
                    handleSubmit(e);
                  }
                }}
                placeholder={error && error.includes('AI credits limit exceeded') ? t('ask.upgradePlaceholder') : getPlaceholderText()}
                className={`live-ask-MeetingCopilot-composer-field w-full bg-gradient-to-br from-[#1A1A1A] via-[#2A2A2A] to-[#333] border-2 border-[#404040] px-5 pr-14 text-white resize-none overflow-hidden placeholder-gray-400 focus:outline-none transition-all duration-300 shadow-lg text-sm leading-relaxed flex items-center ${open ? 'h-[48px] py-3' : 'h-[42px] py-2'}`}
                style={{ borderRadius: '30px', paddingTop: open ? '10px' : '8px' }}
                disabled={isLoading || (error?.includes('AI credits limit exceeded') ?? false)}
                rows={1}
              />
              <div className="absolute right-2 top-1/2 -translate-y-1/2">
                <button
                  type="submit"
                  disabled={isLoading || !input.trim() || (error?.includes('AI credits limit exceeded') ?? false)}
                  className="meeting-assistant-send-button flex h-9 w-9 items-center justify-center rounded-full text-white focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
                  aria-label={t('desktopChat.common.send')}
                >
                  <ArrowUp className="h-4 w-4 text-white" />
                </button>
              </div>
            </div>
          </form>
        </div>
      </div>
    </>
  );
};

export default Bullet;
