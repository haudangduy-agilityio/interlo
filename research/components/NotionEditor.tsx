'use client';

import { EditorContent, useEditor, useEditorState } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import { useEffect, useRef } from 'react';
import { Markdown } from 'tiptap-markdown';
import { useTranslation } from '../../hooks/useTranslation';
import type { NotepadFormat } from '../../lib/notepadFormat';

interface NotionEditorProps {
  content: string;
  setContent: (content: string) => void;
  isActive?: boolean;
  format?: NotepadFormat;
}

// Renders the hint template, replacing {h}/{b}/{s}/{o} with markdown-token chips.
const renderHint = (template: string) => {
  const chips: Record<string, string> = { '{h}': '#', '{b}': '-', '{s}': '*', '{o}': '1.' };
  return template.split(/(\{h\}|\{b\}|\{s\}|\{o\})/).map((part, index) =>
    chips[part] ? (
      <code key={index} className="rounded bg-[#8B8B93]/15 px-1.5 py-0.5 font-mono text-[12px] text-[#9B9BA3]">
        {chips[part]}
      </code>
    ) : (
      part
    )
  );
};

const NotionEditor = ({ content, setContent, isActive, format = 'html' }: NotionEditorProps) => {
  const { t } = useTranslation();
  const lastContent = useRef(content);
  const editor = useEditor({
    extensions: [StarterKit, ...(format === 'markdown' ? [Markdown] : [])],
    content: content,
    editorProps: {
      attributes: {
        class: 'prose prose-sm sm:prose lg:prose-lg xl:prose-2xl mx-auto focus:outline-none',
      },
    },
    onUpdate: ({ editor }) => {
      // Extension/web recordings must remain Markdown for legacy readers.
      const next = format === 'markdown'
        ? editor.storage.markdown.getMarkdown()
        : editor.getHTML();
      lastContent.current = next;
      setContent(next);
    },
  }, [format]);

  useEffect(() => {
    if (!editor || content === lastContent.current) return;
    lastContent.current = content;
    // Loading/refetching notes is not an edit and must not trigger autosave.
    editor.commands.setContent(content, false);
  }, [content, editor]);

  useEffect(() => {
    if (isActive && editor) {
      if (!editor.getText() || editor.view.dom !== document.activeElement) {
        editor.commands.focus('end');
      }
    }
  }, [isActive, editor]);

  const showPlaceholder = useEditorState({
    editor,
    selector: ({ editor: currentEditor }) => {
      if (!currentEditor) return true;
      if (!currentEditor.isEmpty) return false;

      // Markdown shortcuts such as "1. ", "- ", and "# " create an empty
      // structured node. TipTap still calls that document empty, but the user
      // has started composing and the hint must no longer sit behind it.
      return (currentEditor.getJSON().content ?? []).every(
        (node) => node.type === 'paragraph',
      );
    },
  }) ?? true;

  return (
    <div
      className="relative h-full cursor-text overflow-auto bg-transparent"
      onMouseDown={(event) => {
        // Clicking anywhere in the memo card (including the empty area below
        // the text) drops the cursor into the editor.
        if (!(event.target as HTMLElement).closest('.ProseMirror')) {
          event.preventDefault();
          editor?.commands.focus('end');
        }
      }}
    >
      {showPlaceholder && (
        <div className="pointer-events-none absolute left-4 right-4 top-2 z-10">
          {/* Deliberately neutral grays (outside the theme token maps) so this
              reads as a placeholder in both dark and light mode. */}
          <p className="text-base font-bold leading-[1.3] text-[#A2A2AB]">{t('notes.start')}</p>
          <p className="mt-1.5 text-sm leading-6 text-[#9B9BA3]">{renderHint(t('notes.startHint'))}</p>
        </div>
      )}
      <EditorContent
        key={isActive ? 'active' : 'inactive'}
        editor={editor}
        className="min-h-full w-full bg-transparent px-4 pb-4 pt-2 text-white focus:outline-none"
      />
    </div>
  );
};

export default NotionEditor;
