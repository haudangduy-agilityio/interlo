import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { Check, Loader2, Plus, X } from "lucide-react";
import { useAutoscrollToBottom } from "../../hooks/useAutoscrollToBottom";
import { useDesktopChat } from "../../hooks/useDesktopChat";
import { useTranslation } from "../../hooks/useTranslation";
import { toUserErrorMessage } from "../../lib/chat/errors";
import { coerceLanguageCode, languageCodeToLabel } from "../../lib/chat/mappers";
import { type RecordingListItem } from "../../lib/recordingsApi";
import { DesktopChatDetailsPanel } from "./chat/DesktopChatDetailsPanel";
import { DesktopChatEmptyState } from "./chat/DesktopChatEmptyState";
import { DesktopChatMainPanel } from "./chat/DesktopChatMainPanel";
import { DesktopChatSidebar } from "./chat/DesktopChatSidebar";
import { DesktopLanguageSelect } from "./chat/DesktopLanguageSelect";
import {
  DesktopChatTranslateModal,
  type DesktopChatTranslateOpenPayload,
} from "./chat/modals/DesktopChatTranslateModal";
import { DesktopChatMediaTranslateDisabledModal } from "./chat/modals/DesktopChatMediaTranslateDisabledModal";
import { RecentPeopleSuggestions } from "./RecentPeopleSuggestions";
import type { ChatShareLaunchPayload } from "../../lib/liveTranslationInvitationStorage";
import {
  isChatMediaTranslationEnabled,
  mediaTranslationDisabledMessageKey,
} from "./chat/utils/mediaTranslationSettings";

type InviteDraft = {
  id: string;
  name: string;
  email: string;
  language: string;
};

export type DesktopChatLaunchTarget = {
  chatId: string;
  threadParentId?: string | null;
};

export function DesktopChat({
  launchTarget = null,
  onLaunchConsumed,
  onOpenMeeting,
  onOpenLiveSession,
}: {
  launchTarget?: DesktopChatLaunchTarget | null;
  onLaunchConsumed?: () => void;
  onOpenMeeting: (meetingId: string, title?: string) => void;
  onOpenLiveSession: (launch?: ChatShareLaunchPayload) => void;
}) {
  const { t } = useTranslation();
  const {
    currentUser,
    chats,
    selected,
    selectedId,
    activeTab,
    setActiveTab,
    listLoading,
    messagesLoading,
    hasMoreMessages,
    loadingOlder,
    listError,
    sendError,
    replyCounts,
    activeThreadMessageId,
    activeThreadParent,
    threadReplies,
    threadLoading,
    selectChat,
    sendMessage: sendChatMessage,
    sendMeetingNotesMessage,
    editMessage,
    deleteMessage,
    openThread,
    closeThread,
    loadOlderMessages,
    sendThreadReply,
    sendMeetingNotesThreadReply,
    deleteThreadReply,
    editThreadReply,
    togglePin,
    createChatroom,
    notifyTyping,
    presence,
    handleMembersChanged,
    handleLeftOrDeletedChat,
    handleTitleChanged,
    handleMemoryChanged,
    handleMediaTranslationSettingsChanged,
    handleAttachmentTranslationSaved,
    toggleReaction,
    reactionTogglingMessageId,
  } = useDesktopChat({
    enabled: true,
    launchTarget,
    onLaunchConsumed,
  });

  const [viewerLanguage, setViewerLanguage] = useState("English");
  const [rightCollapsed, setRightCollapsed] = useState(true);
  const [rightTab, setRightTab] = useState<"members" | "settings">("members");
  const rightCollapsedBeforeThreadRef = useRef<boolean | null>(null);
  const previousThreadMessageIdRef = useRef<string | null>(null);
  const [modal, setModal] = useState<"new" | "contacts" | null>(null);
  const [newChatTitle, setNewChatTitle] = useState("");
  const [inviteName, setInviteName] = useState("");
  const [inviteEmail, setInviteEmail] = useState("");
  const [inviteLanguage, setInviteLanguage] = useState("en");
  const [inviteSuggestionField, setInviteSuggestionField] = useState<
    "name" | "email" | null
  >(null);
  const [stagedInvites, setStagedInvites] = useState<InviteDraft[]>([]);
  const [creatingChat, setCreatingChat] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [chatSidebarWidth, setChatSidebarWidth] = useState(300);
  const [translateSource, setTranslateSource] =
    useState<DesktopChatTranslateOpenPayload | null>(null);
  const [translateModalOpen, setTranslateModalOpen] = useState(false);
  const [translateDisabledMessage, setTranslateDisabledMessage] = useState<
    string | null
  >(null);

  useEffect(() => {
    if (modal === "new" && currentUser?.language) {
      setInviteLanguage(currentUser.language);
    }
  }, [modal, currentUser?.language]);

  const lastMessageId = selected?.messages[selected.messages.length - 1]?.id;
  const {
    containerRef: messagesScrollRef,
    contentRef: messagesContentRef,
    pinToBottom,
    releaseBottomPin,
  } = useAutoscrollToBottom(
    `${selectedId ?? ""}:${selected?.messages.length ?? 0}:${lastMessageId ?? ""}:${messagesLoading ? "loading" : "ready"}`,
    selectedId,
  );

  useEffect(() => {
    if (activeThreadMessageId) {
      if (!previousThreadMessageIdRef.current) {
        rightCollapsedBeforeThreadRef.current = rightCollapsed;
      }
      setRightCollapsed(false);
    } else if (previousThreadMessageIdRef.current) {
      setRightCollapsed(rightCollapsedBeforeThreadRef.current ?? true);
      rightCollapsedBeforeThreadRef.current = null;
    }
    previousThreadMessageIdRef.current = activeThreadMessageId;
  }, [activeThreadMessageId, rightCollapsed]);

  useEffect(() => {
    if (!selected) return;
    const preferred =
      selected.languages.find((l) => l === "English") ??
      selected.languages[0] ??
      "English";
    setViewerLanguage(preferred);
  }, [selectedId]); // eslint-disable-line react-hooks/exhaustive-deps -- reset view language when switching chats

  const memberNames = useMemo(
    () => (selected?.members ?? []).map((m) => m.name).filter(Boolean),
    [selected?.members],
  );

  const composerMembers = useMemo(
    () =>
      (selected?.members ?? []).map((m) => ({
        id: m.id,
        name: m.name,
        email: m.email || "",
        initials: m.initials,
      })),
    [selected?.members],
  );

  const displayLangCode = useMemo(() => {
    if (selected?.viewerLanguageCode) {
      const match = selected.members.find(
        (m) => languageCodeToLabel(m.languageCode) === viewerLanguage,
      );
      if (match) return coerceLanguageCode(match.languageCode);
      return selected.viewerLanguageCode;
    }
    const member = selected?.members.find(
      (m) => languageCodeToLabel(m.languageCode) === viewerLanguage,
    );
    return coerceLanguageCode(member?.languageCode ?? currentUser?.language);
  }, [selected, viewerLanguage, currentUser?.language]);

  const groupLanguageCodes = useMemo(
    () =>
      Array.from(
        new Set(
          (selected?.members ?? [])
            .map((m) => coerceLanguageCode(m.languageCode))
            .filter(Boolean),
        ),
      ),
    [selected?.members],
  );

  const isBillingOwner = Boolean(
    currentUser &&
      (selected?.billingOwnerId ?? selected?.createdById) === currentUser.id,
  );

  const handleOpenTranslateMedia = (payload: DesktopChatTranslateOpenPayload) => {
    if (!selected) return;
    if (!isChatMediaTranslationEnabled(selected, payload.mode)) {
      const key = mediaTranslationDisabledMessageKey(
        payload.mode,
        isBillingOwner,
      );
      setTranslateDisabledMessage(t(`desktopChat.mediaDisabled.${key}`));
      return;
    }
    setTranslateDisabledMessage(null);
    setTranslateSource(payload);
    setTranslateModalOpen(true);
  };

  const handleOpenThread = (messageId: string) => {
    if (!activeThreadMessageId) {
      rightCollapsedBeforeThreadRef.current = rightCollapsed;
    }
    previousThreadMessageIdRef.current = messageId;
    releaseBottomPin();
    setRightCollapsed(false);
    openThread(messageId);
  };

  const handleComposerSend = (content: string) => {
    pinToBottom("smooth");
    return sendChatMessage(content);
  };

  const handleSendMeetingNotes = async (recordings: RecordingListItem[]) => {
    setSending(true);
    pinToBottom("smooth");
    try {
      for (const recording of recordings) {
        await sendMeetingNotesMessage(recording);
      }
    } finally {
      setSending(false);
    }
  };

  const handleSendThreadMeetingNotes = async (
    recordings: RecordingListItem[],
  ) => {
    for (const recording of recordings) {
      await sendMeetingNotesThreadReply(recording);
    }
  };

  const resetNewChatModal = () => {
    setNewChatTitle("");
    setInviteName("");
    setInviteEmail("");
    setInviteLanguage(currentUser?.language ?? "en");
    setInviteSuggestionField(null);
    setStagedInvites([]);
    setCreateError(null);
    setCreatingChat(false);
  };

  const closeModal = () => {
    setModal(null);
    resetNewChatModal();
  };

  const addStagedInvite = () => {
    const email = inviteEmail.trim().toLowerCase();
    if (!email.includes("@")) return;
    if (stagedInvites.some((invite) => invite.email === email)) {
      setCreateError(t("desktopChat.newChat.emailAlreadyOnList"));
      return;
    }
    setStagedInvites((prev) => [
      ...prev,
      {
        id: crypto.randomUUID(),
        name: inviteName.trim(),
        email,
        language: inviteLanguage,
      },
    ]);
    setInviteName("");
    setInviteEmail("");
    setInviteLanguage(currentUser?.language ?? "en");
    setCreateError(null);
  };

  const createChat = async (event: FormEvent) => {
    event.preventDefault();
    const title = newChatTitle.trim();
    if (!title) {
      setCreateError(t("desktopChat.newChat.enterChatroomName"));
      return;
    }
    const draftEmail = inviteEmail.trim().toLowerCase();
    const draftInvite =
      draftEmail.includes("@") &&
      !stagedInvites.some((invite) => invite.email === draftEmail)
        ? {
            name: inviteName.trim(),
            email: draftEmail,
            language: inviteLanguage,
          }
        : null;
    const invitesToSend = draftInvite
      ? [
          ...stagedInvites.map(({ name, email, language }) => ({
            name,
            email,
            language,
          })),
          draftInvite,
        ]
      : stagedInvites.map(({ name, email, language }) => ({
          name,
          email,
          language,
        }));

    setCreatingChat(true);
    setCreateError(null);
    try {
      await createChatroom(title, invitesToSend);
      closeModal();
    } catch (error) {
      setCreateError(
        toUserErrorMessage(error, t("desktopChat.newChat.failedToCreate")),
      );
      setCreatingChat(false);
    }
  };

  return (
    // The top border separates the workspace from the window chrome, matching
    // the sidebar and conversation-header dividers.
    <div className="desktop-chat-workspace relative flex h-full min-h-0 border-t border-[#2B2B30] bg-[#0B0B0D] text-[#F3F3F5]">
      <DesktopChatSidebar
        chats={chats}
        activeTab={activeTab}
        selectedId={selectedId}
        listLoading={listLoading}
        listError={listError}
        chatHasActiveMember={presence.chatHasActiveMember}
        getDmPeerPresence={presence.getDmPeerPresence}
        getTypingLabel={presence.getTypingLabel}
        onSelectChat={selectChat}
        onChangeTab={setActiveTab}
        onOpenNewChat={() => setModal("new")}
        onWidthChange={setChatSidebarWidth}
      />

      {selected ? (
        <section className="flex min-h-0 min-w-0 flex-1 overflow-hidden bg-[#0B0B0D]">
          <DesktopChatMainPanel
            selected={selected}
            currentUser={currentUser}
            memberNames={memberNames}
            composerMembers={composerMembers}
            viewerLanguage={viewerLanguage}
            onViewerLanguageChange={setViewerLanguage}
            messagesLoading={messagesLoading}
            hasMore={hasMoreMessages}
            loadingOlder={loadingOlder}
            sending={sending}
            sendError={sendError}
            replyCounts={replyCounts}
            activeThreadMessageId={activeThreadMessageId}
            reactionTogglingMessageId={reactionTogglingMessageId}
            scrollRef={messagesScrollRef}
            contentRef={messagesContentRef}
            onTogglePin={togglePin}
            onEditMessage={editMessage}
            onDeleteMessage={(messageId) => void deleteMessage(messageId)}
            onOpenThread={handleOpenThread}
            onToggleReaction={toggleReaction}
            onLoadOlder={loadOlderMessages}
            onOpenTranslateMedia={handleOpenTranslateMedia}
            displayLangCode={displayLangCode}
            onSend={handleComposerSend}
            onSendMeetingNotes={handleSendMeetingNotes}
            onTypingChange={(isTyping) => {
              if (selectedId) notifyTyping(selectedId, isTyping);
            }}
            onOpenMeeting={onOpenMeeting}
            onOpenLiveSession={onOpenLiveSession}
          />

          <DesktopChatDetailsPanel
            selected={selected}
            collapsed={rightCollapsed}
            onCollapsedChange={setRightCollapsed}
            rightTab={rightTab}
            onRightTabChange={setRightTab}
            chatSidebarWidth={chatSidebarWidth}
            currentUser={currentUser}
            composerMembers={composerMembers}
            viewerLanguage={viewerLanguage}
            displayLangCode={displayLangCode}
            sendError={sendError}
            replyCounts={replyCounts}
            activeThreadMessageId={activeThreadMessageId}
            activeThreadParent={activeThreadParent}
            threadReplies={threadReplies}
            threadLoading={threadLoading}
            getActorPresence={(actorId) => presence.getActorPresence(actorId)}
            onCloseThread={closeThread}
            onAddReply={async (content) => {
              await sendThreadReply(content);
            }}
            onDeleteParent={() => {
              if (activeThreadParent) void deleteMessage(activeThreadParent.id);
            }}
            onEditParent={async (content) => {
              if (activeThreadParent) {
                await editMessage(activeThreadParent.id, content);
              }
            }}
            onDeleteReply={(replyId) => void deleteThreadReply(replyId)}
            onEditReply={async (replyId, content) => {
              await editThreadReply(replyId, content);
            }}
            reactionTogglingMessageId={reactionTogglingMessageId}
            onToggleReaction={toggleReaction}
            onTogglePin={togglePin}
            onMembersChanged={handleMembersChanged}
            onLeftChat={handleLeftOrDeletedChat}
            onTitleChanged={handleTitleChanged}
            onMemoryChanged={handleMemoryChanged}
            onMediaTranslationSettingsChanged={
              handleMediaTranslationSettingsChanged
            }
            onChatDeleted={handleLeftOrDeletedChat}
            onOpenTranslateMedia={handleOpenTranslateMedia}
            onOpenMeeting={onOpenMeeting}
            onOpenLiveSession={onOpenLiveSession}
            onSendThreadMeetingNotes={handleSendThreadMeetingNotes}
          />
        </section>
      ) : (
        <DesktopChatEmptyState
          listLoading={listLoading}
          listError={listError}
        />
      )}

      <DesktopChatTranslateModal
        modalOpen={translateModalOpen}
        onClose={() => {
          setTranslateModalOpen(false);
          setTranslateSource(null);
        }}
        source={translateSource}
        billingOwnerId={selected?.billingOwnerId ?? selected?.createdById}
        isBillingOwner={isBillingOwner}
        groupLanguages={groupLanguageCodes}
        onAttachmentTranslationSaved={handleAttachmentTranslationSaved}
      />
      <DesktopChatMediaTranslateDisabledModal
        open={Boolean(translateDisabledMessage)}
        message={translateDisabledMessage ?? ""}
        onClose={() => setTranslateDisabledMessage(null)}
      />

      {modal && (
        <div
          className="absolute inset-0 z-50 flex items-center justify-center bg-black/50 p-6"
          onMouseDown={closeModal}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-label={
              modal === "new"
                ? t("desktopChat.newChat.createChatroomTitle")
                : t("desktopChat.newChat.contactsTitle")
            }
            onMouseDown={(event) => event.stopPropagation()}
            className="flex max-h-[90vh] w-full max-w-md flex-col rounded-2xl border border-[#303036] bg-[#151517] shadow-2xl"
          >
            <header className="flex shrink-0 items-center justify-between border-b border-[#303036] px-5 py-4">
              <h2 className="text-base font-semibold text-[#F1F1F3]">
                {modal === "new"
                  ? t("desktopChat.newChat.createChatroomTitle")
                  : t("desktopChat.newChat.contactsTitle")}
              </h2>
              <button
                type="button"
                onClick={closeModal}
                className="rounded-full p-1.5 text-[#777780] hover:bg-[#202024] hover:text-white"
              >
                <X size={16} />
              </button>
            </header>
            {modal === "new" ? (
              <form
                onSubmit={(event) => void createChat(event)}
                className="flex min-h-0 flex-1 flex-col"
              >
                <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-5 py-4">
                  {createError ? (
                    <p className="text-xs text-red-400">{createError}</p>
                  ) : null}
                  <div>
                    <label className="mb-1.5 block text-xs font-medium text-[#A0A0A8]">
                      {t("desktopChat.newChat.chatroomNameLabel")}
                    </label>
                    <input
                      autoFocus
                      value={newChatTitle}
                      onChange={(event) => setNewChatTitle(event.target.value)}
                      placeholder={t("desktopChat.newChat.chatroomNamePlaceholder")}
                      disabled={creatingChat}
                      className="w-full rounded-lg border border-[#303036] bg-[#19191C] px-3 py-2.5 text-xs text-[#D7D7DB] outline-none placeholder:text-[#66666F] focus:border-[#5156FF]/60 disabled:opacity-50"
                    />
                  </div>
                  <div className="rounded-lg border border-[#303036] bg-[#19191C] p-3">
                    <p className="mb-3 text-[10px] font-semibold uppercase tracking-wide text-[#66666F]">
                      {t("desktopChat.newChat.invitePeopleLabel")}
                    </p>
                    <div className="space-y-2">
                      <div className="relative">
                        <input
                          type="text"
                          value={inviteName}
                          onFocus={() => setInviteSuggestionField("name")}
                          onBlur={() => setInviteSuggestionField(null)}
                          onChange={(event) => setInviteName(event.target.value)}
                          placeholder={t("desktopChat.newChat.fullNamePlaceholder")}
                          disabled={creatingChat}
                          autoComplete="off"
                          className="w-full rounded-lg border border-[#303036] bg-[#151517] px-3 py-2 text-xs text-[#D7D7DB] outline-none placeholder:text-[#66666F] focus:border-[#5156FF]/60 disabled:opacity-50"
                        />
                        <RecentPeopleSuggestions
                          open={inviteSuggestionField === "name"}
                          query={inviteName}
                          excludeEmails={[
                            currentUser?.email ?? "",
                            ...stagedInvites.map((invite) => invite.email),
                          ]}
                          onSelect={(person) => {
                            setInviteName(person.name ?? "");
                            setInviteEmail(person.email);
                            if (person.language) setInviteLanguage(person.language);
                            setInviteSuggestionField(null);
                          }}
                        />
                      </div>
                      <DesktopLanguageSelect
                        value={inviteLanguage}
                        onChange={setInviteLanguage}
                        label={t("desktopChat.newChat.theyllReadMessagesIn")}
                        disabled={creatingChat}
                      />
                      <div className="flex gap-2">
                        <div className="relative min-w-0 flex-1">
                          <input
                            type="email"
                            value={inviteEmail}
                            onFocus={() => setInviteSuggestionField("email")}
                            onBlur={() => setInviteSuggestionField(null)}
                            onChange={(event) =>
                              setInviteEmail(event.target.value)
                            }
                            placeholder={t("desktopChat.newChat.emailPlaceholder")}
                            disabled={creatingChat}
                            autoComplete="off"
                            className="w-full rounded-lg border border-[#303036] bg-[#151517] px-3 py-2 text-xs text-[#D7D7DB] outline-none placeholder:text-[#66666F] focus:border-[#5156FF]/60 disabled:opacity-50"
                          />
                          <RecentPeopleSuggestions
                            open={inviteSuggestionField === "email"}
                            query={inviteEmail}
                            excludeEmails={[
                              currentUser?.email ?? "",
                              ...stagedInvites.map((invite) => invite.email),
                            ]}
                            onSelect={(person) => {
                              setInviteName(person.name ?? "");
                              setInviteEmail(person.email);
                              if (person.language) setInviteLanguage(person.language);
                              setInviteSuggestionField(null);
                            }}
                          />
                        </div>
                        <button
                          type="button"
                          onClick={addStagedInvite}
                          disabled={
                            creatingChat || !inviteEmail.trim().includes("@")
                          }
                          aria-label={t("desktopChat.newChat.addPersonAria")}
                          className="inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-[#5156FF]/40 text-[#A5A7FF] hover:bg-[#5156FF]/10 disabled:opacity-40"
                        >
                          <Plus size={16} />
                        </button>
                      </div>
                    </div>
                    {stagedInvites.length > 0 ? (
                      <ul className="mt-3 space-y-1">
                        {stagedInvites.map((invite) => (
                          <li
                            key={invite.id}
                            className="flex items-center justify-between gap-2 rounded-lg bg-[#151517] px-3 py-2 text-xs text-[#A0A0A8]"
                          >
                            <span className="min-w-0 truncate">
                              {invite.name ? `${invite.name} · ` : ""}
                              {invite.email}
                            </span>
                            <button
                              type="button"
                              disabled={creatingChat}
                              onClick={() =>
                                setStagedInvites((prev) =>
                                  prev.filter((item) => item.id !== invite.id),
                                )
                              }
                              className="shrink-0 text-[11px] font-medium text-red-400 hover:underline disabled:opacity-50"
                            >
                              {t("desktopChat.common.remove")}
                            </button>
                          </li>
                        ))}
                      </ul>
                    ) : null}
                  </div>
                </div>
                <div className="flex shrink-0 justify-end gap-2 border-t border-[#303036] px-5 py-4">
                  <button
                    type="button"
                    onClick={closeModal}
                    disabled={creatingChat}
                    className="rounded-lg px-4 py-2 text-xs font-semibold text-[#85858E] hover:bg-[#202024] disabled:opacity-50"
                  >
                    {t("desktopChat.common.cancel")}
                  </button>
                  <button
                    type="submit"
                    disabled={creatingChat || !newChatTitle.trim()}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-[#5156FF] px-4 py-2 text-xs font-semibold text-white hover:bg-[#484dff] disabled:opacity-40"
                  >
                    {creatingChat ? (
                      <Loader2 size={13} className="animate-spin" />
                    ) : null}
                    {creatingChat
                      ? t("desktopChat.newChat.creating")
                      : t("desktopChat.newChat.create")}
                  </button>
                </div>
              </form>
            ) : (
              <div className="space-y-2 overflow-y-auto px-5 py-4">
                {Array.from(
                  new Map(
                    chats
                      .flatMap((chat) => chat.members)
                      .map((member) => [member.email || member.id, member]),
                  ).values(),
                ).map((member) => (
                  <button
                    key={member.email || member.id}
                    type="button"
                    className="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left hover:bg-[#202024]"
                  >
                    <span className="flex h-9 w-9 items-center justify-center rounded-full bg-[#292955] text-[10px] font-semibold text-[#A5A7FF]">
                      {member.initials}
                    </span>
                    <span className="min-w-0">
                      <span className="block truncate text-xs font-medium text-[#D7D7DB]">
                        {member.name}
                      </span>
                      <span className="block truncate text-[10px] text-[#66666F]">
                        {member.email}
                      </span>
                    </span>
                    {presence.getActorPresence(member.id) === "active" && (
                      <Check size={14} className="ml-auto text-[#67D44B]" />
                    )}
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
