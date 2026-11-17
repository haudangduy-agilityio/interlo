import { productionLogger } from '../../lib/productionLogger'
import axios from 'axios';
import { ArrowUp } from 'lucide-react';
import { Dispatch, SetStateAction, useEffect, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Socket } from 'socket.io-client';
import { useTranslation } from '../../hooks/useTranslation';
import { baseURL } from '../../lib/baseURL';
import { Message } from './AudioRecorder';
import UpgradeNow from './UpgradeNow';

interface CreditUsage {
  used: number;
  limit: number;
}

interface IpcRenderer {
  send: (channel: string, ...args: unknown[]) => void;
}

type IpcWindow = Window & { ipc?: IpcRenderer };

interface AskProps {
  socketRef: React.MutableRefObject<Socket | null>;
  messages: Message[],
  setMessages: Dispatch<SetStateAction<Message[]>>,
  userId: string;
  transcriptData?: unknown[];
  input: string;
  setInput: Dispatch<SetStateAction<string>>;
}

const Ask = ({ messages, setMessages, userId, transcriptData, input, setInput }: AskProps) => {
  const { t } = useTranslation();
  // Input state now managed by parent component
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [_creditUsage, setCreditUsage] = useState<CreditUsage | null>(null);
  const [ipcRenderer, setIpcRenderer] = useState<IpcRenderer | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    // Initialize IPC renderer only on client side
    if (typeof window !== 'undefined') {
      setIpcRenderer((window as IpcWindow).ipc ?? null);
    }
  }, []);

  useEffect(() => {
    // Auto focus the textarea when component mounts
    textareaRef.current?.focus();
  }, []);

  const adjustTextareaHeight = () => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = 'auto';
      textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
    }
  };

  useEffect(() => {
    adjustTextareaHeight();
  }, [input]);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim()) return;

    setError(null);

    // Show user message immediately
    const userMessage = {
      role: 'user' as const,
      content: input,
      timestamp: new Date()
    };
    //productionLogger.debug('User message:', userMessage);
    setMessages(prev => [...prev, userMessage]);

    setIsLoading(true);

    try {
      const response = await axios.post(`${baseURL}/chat/ask`, {
        question: input,
        userId,
        transcriptData,
        chatHistory: messages
      });

      //productionLogger.debug('Response:', response.data);

      if (response.data?.data.error) {
        //productionLogger.error('Error response:', response.data?.data.error);
        setError(response.data?.data.error);
        if (response.data?.data.error === 'AI credits limit exceeded') {
          if (response.data?.data.creditUsage) {
            setCreditUsage(response.data?.data.creditUsage);
          }
        }
      } else {
        // Add assistant's response to messages
        const assistantMessage = {
          role: 'assistant' as const,
          content: response.data.data.answer,
          timestamp: new Date()
        };
        //productionLogger.debug('Assistant response:', assistantMessage);
        setMessages(prev => [...prev, assistantMessage]);

        if (response.data.data.creditUsage) {
          setCreditUsage(response.data.data.creditUsage);
        }
      }
    } catch (error) {
      productionLogger.error('Error sending question:', error.response?.data?.message);
      if (error.response.data?.message.includes('AI credits limit exceeded')) {
        setError('AI credits limit exceeded');
      }
    } finally {
      setIsLoading(false);
      setInput('');
    }
  };

  return (
    <div className="relative flex flex-col h-full bg-background text-white">
      {/* AI Credits Limit Warning at Top */}
      {error && error.includes('AI credits limit exceeded') && (
        <div className="absolute top-0 left-0 right-0 flex flex-col items-center justify-center bg-[#242424] opacity-[.95] rounded-b-[10px] p-8 z-10">
          <h2 className="text-[20px] font-semibold text-[#FBFBFB] mb-4">
            {t("ask.creditLimit")}
          </h2>
          <p className="text-[#FBFBFB] text-[16px] text-center mb-6">
            <span className="block mb-2">
              {t("ask.creditLimitMessage1")} <span className="font-semibold">{t("ask.creditLimitMessage2")}</span> 
            </span>
          </p>
          <div className="flex gap-4">
            <UpgradeNow referrer={'desktop_ask'} />
          </div>
        </div>
      )}
      
      {/* Main Chat Content - Blurred when credits exhausted */}
      <div className={`flex-1 overflow-y-auto p-0 space-y-2 pt-5 ${error && error.includes('AI credits limit exceeded') ? 'blur-sm' : ''}`} style={{paddingBottom: '80px'}}>
        {messages.map((message, index) => (
                <div
                  key={index}
                  className={`flex ${message.role === 'user' ? 'justify-end' : 'justify-start'}`}
                >
                  <div
                    className={`max-w-[98%] p-3 rounded-lg ${message.role === 'user'
                      ? 'bg-primary text-white'
                      : 'bg-gray-800 text-gray-200'
                      }`}
                  >
                    <div className="prose prose-invert max-w-none 
                      prose-li:marker:text-gray-400 
                      prose-li:my-0.5
                      prose-ul:my-1.5
                      prose-ul:ml-0
                      prose-h1:text-2xl
                      prose-h2:text-xl
                      prose-h1:font-bold
                      prose-h2:font-bold
                      prose-h1:mt-4
                      prose-h2:mt-3
                      prose-p:my-2
                      prose-strong:text-[#484DFF]
                      prose-strong:font-bold
                      prose-a:text-[#484DFF]
                      prose-a:underline
                      prose-a:underline-offset-2">
                      <ReactMarkdown
                        remarkPlugins={[remarkGfm]}
                        components={{
                          ul: ({ node: _node, ...props }) => <ul className="list-disc ml-4 space-y-0.5 pl-4" style={{ listStyleType: 'disc', listStylePosition: 'outside' }} {...props} />,
                          li: ({ node: _node, ...props }) => <li className="text-gray-200" style={{ display: 'list-item', listStyleType: 'disc' }} {...props} />,
                          h1: ({ node: _node, ...props }) => <h1 className="text-2xl font-bold mt-4" {...props} />,
                          h2: ({ node: _node, ...props }) => <h2 className="text-xl font-bold mt-3" {...props} />,
                          strong: ({ node: _node, ...props }) => <strong className="text-[#484DFF] font-bold" {...props} />,
                          a: ({ node: _node, href, children }) => {
                            const openLink = () => {
                              productionLogger.debug('🔗 Opening link:', href);
                              if (href && ipcRenderer) {
                                productionLogger.debug('📤 Using IPC to open:', href);
                                ipcRenderer.send("open-link", href);
                              } else if (href) {
                                productionLogger.debug('🌐 Using window.open for:', href);
                                window.open(href, '_blank');
                              }
                            };

                            return (
                              <button
                                type="button"
                                className="text-[#484DFF] underline underline-offset-2 cursor-pointer hover:text-[#6B70FF] font-medium bg-transparent border-none p-0 m-0 inline text-left"
                                onClick={(e) => {
                                  productionLogger.debug('🖱️ Button clicked');
                                  e.preventDefault();
                                  e.stopPropagation();
                                  openLink();
                                }}
                                onMouseDown={(e) => {
                                  productionLogger.debug('🖱️ Mouse down on button');
                                  e.preventDefault();
                                  openLink();
                                }}
                                style={{
                                  all: 'unset',
                                  color: '#484DFF',
                                  textDecoration: 'underline',
                                  textUnderlineOffset: '2px',
                                  cursor: 'pointer',
                                  display: 'inline',
                                  fontSize: 'inherit',
                                  fontFamily: 'inherit',
                                  lineHeight: 'inherit'
                                }}
                              >
                                {children}
                              </button>
                            );
                          }
                        }}
                      >
                        {message.content}
                      </ReactMarkdown>
                    </div>
                  </div>
                </div>
              ))}
              {isLoading && (
                <div className="flex justify-start">
                  <div className="bg-gray-800 text-gray-200 p-1 rounded-lg">
                    <div className="flex space-x-2">
                      <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce"></div>
                      <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: '0.2s' }}></div>
                      <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: '0.4s' }}></div>
                    </div>
                  </div>
                </div>
              )}

              <div ref={messagesEndRef} />
        <div ref={messagesEndRef} />
      </div>
      
      {/* Fixed input at bottom - Blurred when credits exhausted */}
      <div className={`absolute bottom-0 left-0 right-0 p-2 bg-background ${error && error.includes('AI credits limit exceeded') ? 'blur-sm' : ''}`}>
              {messages.length === 0 && !isLoading && (
                <div className="mb-4 text-center">
                  <p className="text-2xl text-gray-400">{t("ask.label")}</p>
                </div>
              )}
              <form onSubmit={handleSubmit}>
                <div className="relative">
                  <textarea
                    ref={textareaRef}
                    value={isLoading ? t('ask.thinking') : input}
                    onChange={(e) => setInput(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && !e.shiftKey) {
                        e.preventDefault();
                        handleSubmit(e);
                      }
                    }}
                    placeholder={error && error.includes('AI credits limit exceeded') ? t('ask.upgradePlaceholder') : t("ask.placeholder")}
                    className="w-full p-3.5 pr-10 bg-[#2A2A2A] text-white rounded-2xl focus:outline-none focus:ring-2 focus:ring-primary resize-none overflow-hidden min-h-[40px] max-h-[200px] transition-all"
                    disabled={isLoading || (error?.includes('AI credits limit exceeded') ?? false)}
                    rows={1}
                    autoFocus
                  />
                  <button
                    type="submit"
                    disabled={isLoading || !input.trim() || (error?.includes('AI credits limit exceeded') ?? false)}
                    className="absolute right-2 top-1/2 transform -translate-y-1/2 bg-primary p-1.5 rounded-full hover:bg-primary/80 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    <ArrowUp className="w-5 h-5 text-white" />
                  </button>
                </div>
        </form>
      </div>
    </div>
  );
};

export default Ask; 
