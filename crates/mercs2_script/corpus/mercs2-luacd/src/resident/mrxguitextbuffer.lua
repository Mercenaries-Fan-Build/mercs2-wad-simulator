import("MrxGui")
import("MrxGuiBase")
import("MrxGuiManager")
local kScrollSpeed = 50

function HandleInstantiationEventForTextBuffer(oWidget, tEvent)
  local bFlowDown = false
  local bHasBackdrop = false
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local nHeight = nY2 - nY1
  local nWidth = nX2 - nX1
  if "MessageBox" == oWidget.BasicData.name then
    bHasBackdrop = true
    oWidget:SetTranslucency(128)
  end
  oWidget.CustomData.sTextFont = "english_18"
  oWidget.CustomData.nTextScale = 1
  oWidget.CustomData.bFlowDown = bFlowDown or false
  oWidget.CustomData.bAdvancing = false
  oWidget.CustomData.nAdvanceSpeed = -1 * kScrollSpeed
  oWidget.CustomData.nAdvanceDistanceRemaining = 0
  if bFlowDown then
    oWidget.CustomData.nAdvanceSpeed = oWidget.CustomData.nAdvanceSpeed * -1
  end
  if not bHasBackdrop then
    oWidget:SetTranslucency(0)
  end
  oWidget.CustomData.bHasBackdrop = bHasBackdrop
  oWidget.CustomData.nBorder = 0
  if bHasBackdrop then
    oWidget.CustomData.nBorder = 12
    oWidget.CustomData.tBackdropWidgets = oWidget:GetChildren()
  end
  oWidget.CustomData.nWidth = (nWidth or 300) - oWidget.CustomData.nBorder * 2
  oWidget.CustomData.nHeight = (nHeight or 100) - oWidget.CustomData.nBorder * 2
  oWidget.CustomData.x1 = nX1 + oWidget.CustomData.nBorder
  oWidget.CustomData.x2 = nX2 - oWidget.CustomData.nBorder
  oWidget.CustomData.y1 = nY1 + oWidget.CustomData.nBorder
  oWidget.CustomData.y2 = nY2 - oWidget.CustomData.nBorder
  oWidget.CustomData.nRemainingSpace = oWidget.CustomData.nHeight
  oWidget.CustomData.CurrentMessages = {}
  oWidget.CustomData.PendingMessages = {
    [1] = {},
    [2] = {},
    [3] = {},
    [4] = {},
    [5] = {}
  }
  oWidget.CustomData.MessageIndex = {}
  oWidget.CustomData.nNextMessageId = 1
  oWidget:SetEventHandler("GuiUpdate", HandleTextBufferUpdateEvent)
  oWidget.AddMessage = AddMessage
  oWidget.GetCurrentMessageId = GetCurrentMessageId
  oWidget.ClearMessages = ClearMessages
  oWidget.ClearVisibleMessages = ClearVisibleMessages
  oWidget.RemovePendingMessage = RemovePendingMessage
  oWidget.ModifyPendingMessage = ModifyPendingMessage
  oWidget.SetLocation = SetLocation
  oWidget:SetVisible(false)
end

function InstantiateTextBuffer(nX, nY, nWidth, nHeight, bFlowDown, bHasBackdrop)
  nX = ValidateParameter(nX, "number", nil)
  nY = ValidateParameter(nY, "number", nil)
  nWidth = ValidateParameter(nWidth, "number", 300)
  nHeight = ValidateParameter(nHeight, "number", 100)
  bFlowDown = ValidateParameter(bFlowDown, "boolean", false)
  bHasBackdrop = ValidateParameter(bHasBackdrop, "boolean", true)
  if not nX or not nY then
    return
  end
  NewTextBuffer = MrxGui.ImageWidget:new()
  NewTextBuffer:SetLocation(nX, nY, nX + (nWidth or 300), nY + (nHeight or 100))
  NewTextBuffer.CustomData.sTextFont = "english_18"
  NewTextBuffer.CustomData.nTextScale = 1
  NewTextBuffer.CustomData.bFlowDown = bFlowDown or false
  NewTextBuffer.CustomData.bAdvancing = false
  NewTextBuffer.CustomData.nAdvanceSpeed = -1 * kScrollSpeed
  NewTextBuffer.CustomData.nAdvanceDistanceRemaining = 0
  if bFlowDown then
    NewTextBuffer.CustomData.nAdvanceSpeed = NewTextBuffer.CustomData.nAdvanceSpeed * -1
  end
  if bHasBackdrop then
    NewTextBuffer:SetColor(16, 16, 32)
    NewTextBuffer:SetTranslucency(192)
  else
    NewTextBuffer:SetTranslucency(0)
  end
  NewTextBuffer.CustomData.bHasBackdrop = bHasBackdrop
  NewTextBuffer.CustomData.nBorder = 0
  if bHasBackdrop then
    NewTextBuffer.CustomData.nBorder = 20
  end
  local nNewX1, nNewY1, nNewX2, nNewY2 = NewTextBuffer:GetLocation()
  NewTextBuffer.CustomData.nWidth = (nWidth or 300) - NewTextBuffer.CustomData.nBorder * 2
  NewTextBuffer.CustomData.nHeight = (nHeight or 100) - NewTextBuffer.CustomData.nBorder * 2
  NewTextBuffer.CustomData.x1 = nNewX1 + NewTextBuffer.CustomData.nBorder
  NewTextBuffer.CustomData.x2 = nNewX2 - NewTextBuffer.CustomData.nBorder
  NewTextBuffer.CustomData.y1 = nNewY1 + NewTextBuffer.CustomData.nBorder
  NewTextBuffer.CustomData.y2 = nNewY2 - NewTextBuffer.CustomData.nBorder
  NewTextBuffer.CustomData.nRemainingSpace = NewTextBuffer.CustomData.nHeight
  NewTextBuffer.CustomData.CurrentMessages = {}
  NewTextBuffer.CustomData.PendingMessages = {
    [1] = {},
    [2] = {},
    [3] = {},
    [4] = {},
    [5] = {}
  }
  oWidget.CustomData.MessageIndex = {}
  oWidget.CustomData.nNextMessageId = 1
  NewTextBuffer:SetEventHandler("GuiUpdate", HandleTextBufferUpdateEvent)
  NewTextBuffer.AddMessage = AddMessage
  NewTextBuffer.GetCurrentMessageId = GetCurrentMessageId
  NewTextBuffer.ClearMessages = ClearMessages
  NewTextBuffer.ClearVisibleMessages = ClearVisibleMessages
  NewTextBuffer.RemovePendingMessage = RemovePendingMessage
  NewTextBuffer.ModifyPendingMessage = ModifyPendingMessage
  NewTextBuffer.SetLocation = SetLocation
  NewTextBuffer:SetVisible(false)
  MrxGui.AddWidget(NewTextBuffer)
  return NewTextBuffer
end

function SetLocation(oTextBuffer, nX1, nY1, nX2, nY2)
  MrxGui.ImageWidget.SetLocation(oTextBuffer, nX1, nY1, nX2, nY2)
  local nNewX1, nNewY1, nNewX2, nNewY2 = oTextBuffer:GetLocation()
  oTextBuffer.CustomData.x1 = nNewX1 + oTextBuffer.CustomData.nBorder
  oTextBuffer.CustomData.x2 = nNewX2 - oTextBuffer.CustomData.nBorder
  oTextBuffer.CustomData.y1 = nNewY1 + oTextBuffer.CustomData.nBorder
  oTextBuffer.CustomData.y2 = nNewY2 - oTextBuffer.CustomData.nBorder
end

function AddMessage(oTextBuffer, sMessage, nPriority, nDisplayDuration, nFadeDuration, bClearBuffer, bAllowsAppends, fCallback, tCallbackData)
  if "table" ~= type(oTextBuffer) or "string" ~= type(sMessage) then
    return nil
  end
  nPriority = ValidateParameter(nPriority, "number", 5)
  nDisplayDuration = ValidateParameter(nDisplayDuration, "number", 2)
  nFadeDuration = ValidateParameter(nFadeDuration, "number", 0.25)
  bClearBuffer = ValidateParameter(bClearBuffer, "boolean", false)
  bAllowsAppends = ValidateParameter(bAllowsAppends, "boolean", true)
  fCallback = ValidateParameter(fCallback, "function", nil)
  tCallbackData = ValidateParameter(tCallbackData, "table", {})
  NewTextMessage = MrxGui.TextWidget:new()
  if "PDA Subtitle Buffer" ~= oTextBuffer.BasicData.name then
    MrxGuiManager.AddWidgetToHud(oTextBuffer:GetOwner(), NewTextMessage)
  end
  if "Subtitle Buffer" ~= oTextBuffer.BasicData.name and "PDA Subtitle Buffer" ~= oTextBuffer.BasicData.name then
    NewTextMessage:SetOwner(oTextBuffer:GetOwner())
  end
  NewTextMessage:SetFont(oTextBuffer.CustomData.sTextFont)
  NewTextMessage:SetScale(oTextBuffer.CustomData.nTextScale)
  NewTextMessage:SetText(sMessage)
  NewTextMessage.CustomData.bClearBuffer = bClearBuffer or false
  if nil == bAllowsAppends then
    NewTextMessage.CustomData.bAllowsAppends = true
  else
    NewTextMessage.CustomData.bAllowsAppends = bAllowsAppends
  end
  NewTextMessage.CustomData.nDisplayDuration = nDisplayDuration or 2
  NewTextMessage.CustomData.nFadeDuration = nFadeDuration or 0.5
  NewTextMessage:SetLocation(oTextBuffer.CustomData.x1, oTextBuffer.CustomData.y1, oTextBuffer.CustomData.x2, oTextBuffer.CustomData.y2)
  NewTextMessage:Wrap()
  NewTextMessage.CustomData.nHeight = GetMessageHeight(NewTextMessage)
  NewTextMessage.CustomData.bNeedsScrolling = true
  NewTextMessage.CustomData.fCallback = fCallback
  NewTextMessage.CustomData.tCallbackData = tCallbackData
  NewTextMessage.CallCallback = CallCallback
  if nDisplayDuration < 0 then
    NewTextMessage.CustomData.nDisplayDuration = 10000
    NewTextMessage.CustomData.bPersistent = true
  end
  nPriority = nPriority or 5
  if "number" ~= type(nPriority) or nPriority < 0 or 5 < nPriority then
    nPriority = 5
  end
  NewTextMessage.ParentWidget = oTextBuffer
  if 0 == nPriority then
    local nPredictedRemainingSpace = oTextBuffer.CustomData.nRemainingSpace
    local nIndex = 1
    while nPredictedRemainingSpace < NewTextMessage.CustomData.nHeight and oTextBuffer.CustomData.CurrentMessages[nIndex] do
      oTextBuffer.CustomData.CurrentMessages[nIndex].CustomData.nDisplayDuration = 0
      oTextBuffer.CustomData.CurrentMessages[nIndex].CustomData.bPersistent = nil
      oTextBuffer.CustomData.CurrentMessages[nIndex].CustomData.nFadeDuration = 0
      nPredictedRemainingSpace = nPredictedRemainingSpace + oTextBuffer.CustomData.CurrentMessages[nIndex].CustomData.nHeight
      nIndex = nIndex + 1
    end
    NewTextMessage.CustomData.bZeroPriority = true
    local oCheckForZeroMessage = oTextBuffer.CustomData.PendingMessages[1][1]
    while oCheckForZeroMessage and oCheckForZeroMessage.CustomData.bZeroPriority do
      table.remove(oTextBuffer.CustomData.PendingMessages[1], 1)
      if oCheckForZeroMessage.CustomData.nId then
        oTextBuffer.CustomData.MessageIndex[oCheckForZeroMessage.CustomData.nId] = nil
      end
      MrxGuiManager.RemoveWidgetFromHud(oTextBuffer:GetOwner(), oCheckForZeroMessage)
      MrxGui.RemoveWidget(oCheckForZeroMessage)
      oCheckForZeroMessage:delete()
      oCheckForZeroMessage = oTextBuffer.CustomData.PendingMessages[1][1]
    end
    table.insert(oTextBuffer.CustomData.PendingMessages[1], 1, NewTextMessage)
    local bMessagesRemainingToBePushed = true
    while bMessagesRemainingToBePushed do
      bMessagesRemainingToBePushed = PushMessageIntoTextBuffer(oTextBuffer)
    end
  else
    table.insert(oTextBuffer.CustomData.PendingMessages[nPriority], NewTextMessage)
    local nId = oTextBuffer.CustomData.nNextMessageId
    oTextBuffer.CustomData.MessageIndex[nId] = NewTextMessage
    NewTextMessage.CustomData.nId = nId
    NewTextMessage.CustomData.nPriority = nPriority
    oTextBuffer.CustomData.nNextMessageId = nId + 1
    local bMessagesRemainingToBePushed = true
    while bMessagesRemainingToBePushed do
      bMessagesRemainingToBePushed = PushMessageIntoTextBuffer(oTextBuffer)
    end
  end
  if oTextBuffer.CustomData.bHasBackdrop then
    oTextBuffer:SetVisible(true)
  end
  return NewTextMessage.CustomData.nId
end

function CallCallback(oMessage)
  if oMessage.CustomData.fCallback then
    if "table" ~= type(oMessage.CustomData.tCallbackData) then
      oMessage.CustomData.tCallbackData = {}
    end
    oMessage.CustomData.fCallback(unpack(oMessage.CustomData.tCallbackData))
    oMessage.CustomData.fCallback = nil
  end
end

function AdvanceMessages(oTextBuffer)
  if oTextBuffer.CustomData.CurrentMessages[1] then
    oTextBuffer.CustomData.CurrentMessages[1].CustomData.nDisplayDuration = 0
    oTextBuffer.CustomData.CurrentMessages[1].CustomData.bPersistent = nil
  end
end

function GetCurrentMessageId(oTextBuffer)
  visibleIds = {}
  for nIndex, oMessage in pairs(oTextBuffer.CustomData.CurrentMessages) do
    if oMessage.CustomData.nId then
      table.insert(visibleIds, oMessage.CustomData.nId)
    end
  end
  return visibleIds
end

function ClearMessages(oTextBuffer)
  oTextBuffer.CustomData.bAdvancing = false
  oTextBuffer.CustomData.nAdvanceDistanceRemaining = 0
  oTextBuffer.CustomData.nRemainingSpace = oTextBuffer.CustomData.nHeight
  for nIndex, oMessage in pairs(oTextBuffer.CustomData.CurrentMessages) do
    MrxGuiManager.RemoveWidgetFromHud(oTextBuffer:GetOwner(), oMessage)
    MrxGui.RemoveWidget(oMessage)
    oMessage:delete()
    oTextBuffer.CustomData.CurrentMessages[nIndex] = nil
  end
  oTextBuffer.CustomData.CurrentMessages = {}
  oTextBuffer:RemoveAllChildren()
  if oTextBuffer.CustomData.bHasBackdrop and oTextBuffer.CustomData.tBackdropWidgets then
    for n, oBackdropWidget in pairs(oTextBuffer.CustomData.tBackdropWidgets) do
      oTextBuffer:AddChild(oBackdropWidget)
    end
  end
  local nIndex = 1
  while oTextBuffer.CustomData.PendingMessages[nIndex] do
    oTextBuffer.CustomData.PendingMessages[nIndex] = {}
    nIndex = nIndex + 1
  end
  oTextBuffer.CustomData.MessageIndex = {}
  oTextBuffer:SetVisible(false)
end

function ClearVisibleMessages(oTextBuffer, bAdvance)
  oTextBuffer.CustomData.bAdvancing = false
  oTextBuffer.CustomData.nAdvanceDistanceRemaining = 0
  oTextBuffer.CustomData.nRemainingSpace = oTextBuffer.CustomData.nHeight
  for nIndex, oMessage in pairs(oTextBuffer.CustomData.CurrentMessages) do
    MrxGuiManager.RemoveWidgetFromHud(oTextBuffer:GetOwner(), oMessage)
    MrxGui.RemoveWidget(oMessage)
    oMessage:delete()
    oTextBuffer.CustomData.CurrentMessages[nIndex] = nil
  end
  oTextBuffer.CustomData.CurrentMessages = {}
  oTextBuffer:RemoveAllChildren()
  if oTextBuffer.CustomData.bHasBackdrop and oTextBuffer.CustomData.tBackdropWidgets then
    for n, oBackdropWidget in pairs(oTextBuffer.CustomData.tBackdropWidgets) do
      oTextBuffer:AddChild(oBackdropWidget)
    end
  end
  if bAdvance then
    local bMessagesRemainingToBePushed = true
    while bMessagesRemainingToBePushed do
      bMessagesRemainingToBePushed = PushMessageIntoTextBuffer(oTextBuffer)
    end
  end
  if 0 == #oTextBuffer.CustomData.CurrentMessages then
    oTextBuffer:SetVisible(false)
  end
end

function ModifyPendingMessage(oTextBuffer, nMessageId, sMessage, nDisplayDuration, nFadeDuration, bClearBuffer, bAllowsAppends, fCallback, tCallbackData)
  local oMessage = oTextBuffer.CustomData.MessageIndex[nMessageId]
  if oMessage and oMessage.CustomData.nPriority then
    local nIndex
    for n, oCurrentMessage in pairs(oTextBuffer.CustomData.PendingMessages[oMessage.CustomData.nPriority]) do
      if oCurrentMessage.CustomData.nId == oMessage.CustomData.nId then
        nIndex = n
      end
    end
    if nIndex then
      sMessage = ValidateParameter(sMessage, "string", nil)
      nDisplayDuration = ValidateParameter(nDisplayDuration, "number", nil)
      nFadeDuration = ValidateParameter(nFadeDuration, "number", nil)
      bClearBuffer = ValidateParameter(bClearBuffer, "boolean", nil)
      bAllowsAppends = ValidateParameter(bAllowsAppends, "boolean", nil)
      fCallback = ValidateParameter(fCallback, "function", nil)
      tCallbackData = ValidateParameter(tCallbackData, "table", nil)
      if sMessage then
        oMessage:SetText(sMessage)
        oMessage:Wrap()
        oMessage.CustomData.nHeight = GetMessageHeight(oMessage)
      end
      oMessage.CustomData.bClearBuffer = bClearBuffer or oMessage.CustomData.bClearBuffer
      oMessage.CustomData.bAllowsAppends = bAllowsAppends or oMessage.CustomData.bAllowsAppends
      oMessage.CustomData.nDisplayDuration = nDisplayDuration or oMessage.CustomData.nDisplayDuration
      oMessage.CustomData.nFadeDuration = nFadeDuration or oMessage.CustomData.nFadeDuration
      oMessage.CustomData.fCallback = fCallback or oMessage.CustomData.fCallback
      oMessage.CustomData.tCallbackData = tCallbackData or oMessage.CustomData.tCallbackData or {}
      if oMessage.CustomData.nDisplayDuration < 0 then
        oMessage.CustomData.bPersistent = true
      else
        oMessage.CustomData.bPersistent = nil
      end
      return true
    end
  end
  return false
end

function RemovePendingMessage(oTextBuffer, nMessageId)
  local oMessage = oTextBuffer.CustomData.MessageIndex[nMessageId]
  if oMessage and oMessage.CustomData.nPriority then
    local nIndex
    for n, oCurrentMessage in pairs(oTextBuffer.CustomData.PendingMessages[oMessage.CustomData.nPriority]) do
      if oCurrentMessage.CustomData.nId == oMessage.CustomData.nId then
        nIndex = n
      end
    end
    if nIndex then
      table.remove(oTextBuffer.CustomData.PendingMessages[oMessage.CustomData.nPriority], nIndex)
      oTextBuffer.CustomData.MessageIndex[nMessageId] = nil
      MrxGuiManager.RemoveWidgetFromHud(oTextBuffer:GetOwner(), oMessage)
      MrxGui.RemoveWidget(oMessage)
      oMessage:delete()
      return true
    end
  end
  return false
end

function HandleTextBufferUpdateEvent(oTextBuffer, nTimeSinceLastUpdate)
  local oExitingMessage = oTextBuffer.CustomData.CurrentMessages[0]
  if oExitingMessage then
    local nTranslucencyDecrement
    if 0 >= oExitingMessage.CustomData.nFadeDuration then
      nTranslucencyDecrement = 9999
    else
      nTranslucencyDecrement = 256 / oExitingMessage.CustomData.nFadeDuration * nTimeSinceLastUpdate
    end
    if nTranslucencyDecrement > oExitingMessage:GetTranslucency() then
      MrxGuiManager.RemoveWidgetFromHud(oTextBuffer:GetOwner(), oExitingMessage)
      MrxGui.RemoveWidget(oExitingMessage)
      oExitingMessage:delete()
      oTextBuffer.CustomData.CurrentMessages[0] = nil
      if IsEmpty(oTextBuffer.CustomData.CurrentMessages) then
        oTextBuffer:SetVisible(false)
      end
    else
      oExitingMessage:SetTranslucency(oExitingMessage:GetTranslucency() - nTranslucencyDecrement)
    end
  end
  local nCurrentMessageIndex = 1
  while oTextBuffer.CustomData.CurrentMessages[nCurrentMessageIndex] do
    local oCurrentMessage = oTextBuffer.CustomData.CurrentMessages[nCurrentMessageIndex]
    if oCurrentMessage:GetTranslucency() < 255 then
      local nTranslucencyIncrement
      if 0 >= oCurrentMessage.CustomData.nFadeDuration then
        nTranslucencyIncrement = 9999
      else
        nTranslucencyIncrement = 255 / oCurrentMessage.CustomData.nFadeDuration * nTimeSinceLastUpdate
      end
      oCurrentMessage:SetTranslucency(oCurrentMessage:GetTranslucency() + nTranslucencyIncrement)
      if oCurrentMessage:GetTranslucency() > 255 then
        oCurrentMessage:SetTranslucency(255)
      end
    end
    nCurrentMessageIndex = nCurrentMessageIndex + 1
  end
  if oTextBuffer.CustomData.bAdvancing then
    local oCurrentMessage = oTextBuffer.CustomData.CurrentMessages[1]
    if oCurrentMessage and not oCurrentMessage.CustomData.bPersistent then
      oCurrentMessage.CustomData.nDisplayDuration = oCurrentMessage.CustomData.nDisplayDuration - nTimeSinceLastUpdate
      if 0 > oCurrentMessage.CustomData.nDisplayDuration then
        oCurrentMessage.CustomData.nDisplayDuration = 0
      end
    end
    local nAdvanceDistance = nTimeSinceLastUpdate * oTextBuffer.CustomData.nAdvanceSpeed
    if MboxAbs(nAdvanceDistance) >= oTextBuffer.CustomData.nAdvanceDistanceRemaining then
      if nAdvanceDistance < 0 then
        nAdvanceDistance = oTextBuffer.CustomData.nAdvanceDistanceRemaining * -1
      else
        nAdvanceDistance = oTextBuffer.CustomData.nAdvanceDistanceRemaining
      end
      oTextBuffer.CustomData.nAdvanceDistanceRemaining = 0
      oTextBuffer.CustomData.bAdvancing = false
    else
      oTextBuffer.CustomData.nAdvanceDistanceRemaining = oTextBuffer.CustomData.nAdvanceDistanceRemaining - MboxAbs(nAdvanceDistance)
    end
    for nIndex in pairs(oTextBuffer.CustomData.CurrentMessages) do
      oTextBuffer.CustomData.CurrentMessages[nIndex]:OffsetLocation(0, nAdvanceDistance)
    end
    if IsEmpty(oTextBuffer.CustomData.CurrentMessages) and nil == oTextBuffer.CustomData.CurrentMessages[0] then
      oTextBuffer.CustomData.nAdvanceDistanceRemaining = 0
      oTextBuffer.bAdvancing = false
    end
  else
    local oCurrentMessage = oTextBuffer.CustomData.CurrentMessages[1]
    if oCurrentMessage then
      if nTimeSinceLastUpdate >= oCurrentMessage.CustomData.nDisplayDuration and not oCurrentMessage.CustomData.bPersistent then
        if oTextBuffer.CustomData.CurrentMessages[0] then
          MrxGuiManager.RemoveWidgetFromHud(oTextBuffer:GetOwner(), oTextBuffer.CustomData.CurrentMessages[0])
          MrxGui.RemoveWidget(oTextBuffer.CustomData.CurrentMessages[0])
          oTextBuffer.CustomData.CurrentMessages[0]:delete()
          oTextBuffer.CustomData.CurrentMessages[0] = nil
        end
        table.remove(oTextBuffer.CustomData.CurrentMessages, 1)
        oTextBuffer.CustomData.CurrentMessages[0] = oCurrentMessage
        oTextBuffer.CustomData.nAdvanceDistanceRemaining = oCurrentMessage.CustomData.nHeight
        oTextBuffer.CustomData.nRemainingSpace = oTextBuffer.CustomData.nRemainingSpace + oCurrentMessage.CustomData.nHeight
        local bMessagesRemainingToBePushed = true
        while bMessagesRemainingToBePushed do
          bMessagesRemainingToBePushed = PushMessageIntoTextBuffer(oTextBuffer)
        end
        oTextBuffer.CustomData.bAdvancing = true
      elseif not oCurrentMessage.CustomData.bPersistent then
        oCurrentMessage.CustomData.nDisplayDuration = oCurrentMessage.CustomData.nDisplayDuration - nTimeSinceLastUpdate
      end
    end
  end
end

function PushMessageIntoTextBuffer(oTextBuffer)
  local nLastMessageIndex = 0
  while oTextBuffer.CustomData.CurrentMessages[nLastMessageIndex + 1] do
    nLastMessageIndex = nLastMessageIndex + 1
  end
  local oPreviousMessage
  if 0 < nLastMessageIndex then
    oPreviousMessage = oTextBuffer.CustomData.CurrentMessages[nLastMessageIndex]
    if oPreviousMessage and not oPreviousMessage.CustomData.bAllowsAppends then
      return false
    end
  end
  local bEmpty = false
  if 0 == table.getn(oTextBuffer.CustomData.CurrentMessages) then
    bEmpty = true
  end
  local oFoundMessage
  local nPriorityIndex = 1
  local nPriorityOfMessage = 1
  local nIndexOfMessage = 1
  while not oFoundMessage and nPriorityIndex < 6 do
    oFoundMessage = oTextBuffer.CustomData.PendingMessages[nPriorityIndex][1]
    nPriorityOfMessage = nPriorityIndex
    nIndexOfMessage = 1
    nPriorityIndex = nPriorityIndex + 1
  end
  if not oFoundMessage then
    return false
  end
  if oFoundMessage.CustomData.nHeight > oTextBuffer.CustomData.nRemainingSpace then
    if not oTextBuffer.CustomData.nOneLineHeight then
      oTest = MrxGui.TextWidget:new()
      oTest:SetFont(oTextBuffer.CustomData.sTextFont)
      oTest:SetScale(oTextBuffer.CustomData.nTextScale)
      oTest:SetText("Test")
      oTextBuffer.CustomData.nOneLineHeight = oTest:GetHeight()
      oTest:delete()
    end
    if oTextBuffer.CustomData.nOneLineHeight > oTextBuffer.CustomData.nRemainingSpace then
      return false
    end
    if oFoundMessage.CustomData.nHeight > oTextBuffer.CustomData.nHeight then
      local tLines = oFoundMessage:SplitIntoLines()
      local nLineDisplayDuration = oFoundMessage.CustomData.nDisplayDuration / #tLines
      for nIndex, oLineMessage in pairs(tLines) do
        oLineMessage:SetOwner(oFoundMessage:GetOwner())
        oLineMessage:SetScale(oFoundMessage:GetScale())
        oLineMessage.CustomData.bAllowsAppends = true
        oLineMessage.CustomData.bClearBuffer = false
        oLineMessage.CustomData.nDisplayDuration = nLineDisplayDuration
        oLineMessage.CustomData.nFadeDuration = oFoundMessage.CustomData.nFadeDuration * 0.5
        oLineMessage.CustomData.bNeedsScrolling = true
        oLineMessage.CustomData.nHeight = GetMessageHeight(oLineMessage)
        oLineMessage.CustomData.nPriority = nPriorityOfMessage
        oLineMessage.CallCallback = CallCallback
        MrxGui.RemoveWidget(oLineMessage)
        if "PDA Subtitle Buffer" ~= oTextBuffer.BasicData.name then
          MrxGuiManager.AddWidgetToHud(oTextBuffer:GetOwner(), oLineMessage)
        end
      end
      if tLines[1] then
        tLines[1].CustomData.bClearBuffer = oFoundMessage.CustomData.bClearBuffer
        tLines[1].CustomData.fCallback = oFoundMessage.CustomData.fCallback
        tLines[1].CustomData.tCallbackData = oFoundMessage.CustomData.tCallbackData
        tLines[1].CustomData.nDisplayDuration = nLineDisplayDuration * 1.5
      end
      if tLines[#tLines] then
        tLines[#tLines].CustomData.bAllowsAppends = oFoundMessage.CustomData.bAllowsAppends
        tLines[#tLines].CustomData.nFadeDuration = oFoundMessage.CustomData.nFadeDuration
        tLines[#tLines].CustomData.nDisplayDuration = nLineDisplayDuration * 0.5
      end
      table.remove(oTextBuffer.CustomData.PendingMessages[nPriorityOfMessage], nIndexOfMessage)
      if oFoundMessage.CustomData.nId then
        oTextBuffer.CustomData.MessageIndex[oFoundMessage.CustomData.nId] = nil
      end
      local nIndex = #tLines
      while 0 < nIndex do
        table.insert(oTextBuffer.CustomData.PendingMessages[nPriorityOfMessage], 1, tLines[nIndex])
        nIndex = nIndex - 1
      end
      MrxGuiManager.RemoveWidgetFromHud(oTextBuffer:GetOwner(), oFoundMessage)
      oFoundMessage:delete()
      oFoundMessage = tLines[1]
    else
      return false
    end
  end
  if not oFoundMessage then
    return false
  end
  if oFoundMessage.CustomData.bClearBuffer then
    ClearVisibleMessages(oTextBuffer, false)
    oPreviousMessage = nil
  end
  oFoundMessage:SetTranslucency(0)
  oPreviousMessage = oTextBuffer.CustomData.CurrentMessages[nLastMessageIndex]
  if oPreviousMessage then
    local nPrevX, nPrevY = oPreviousMessage:GetLocation()
    if oTextBuffer.CustomData.bFlowDown then
      oFoundMessage:SetLocation(oTextBuffer.CustomData.x1, nPrevY - oFoundMessage.CustomData.nHeight, oTextBuffer.CustomData.x2, nPrevY)
    else
      oFoundMessage:SetLocation(oTextBuffer.CustomData.x1, nPrevY + oPreviousMessage.CustomData.nHeight, oTextBuffer.CustomData.x2, nPrevY + oPreviousMessage.CustomData.nHeight + oFoundMessage.CustomData.nHeight)
    end
  elseif oTextBuffer.CustomData.bFlowDown then
    oFoundMessage:SetLocation(oTextBuffer.CustomData.x1, oTextBuffer.CustomData.y2 - oFoundMessage.CustomData.nHeight, oTextBuffer.CustomData.x2, oTextBuffer.CustomData.y2)
  else
    oFoundMessage:SetLocation(oTextBuffer.CustomData.x1, oTextBuffer.CustomData.y1, oTextBuffer.CustomData.x2, oTextBuffer.CustomData.y1 + oFoundMessage.CustomData.nHeight)
  end
  table.remove(oTextBuffer.CustomData.PendingMessages[nPriorityOfMessage], nIndexOfMessage)
  if oFoundMessage.CustomData.nId then
    oTextBuffer.CustomData.MessageIndex[oFoundMessage.CustomData.nId] = nil
  end
  table.insert(oTextBuffer.CustomData.CurrentMessages, oFoundMessage)
  oTextBuffer:AddChild(oFoundMessage)
  MrxGui.AddWidget(oFoundMessage)
  oFoundMessage:SetSleeping(false)
  oTextBuffer.CustomData.nRemainingSpace = oTextBuffer.CustomData.nRemainingSpace - oFoundMessage.CustomData.nHeight
  oFoundMessage:CallCallback()
  if oTextBuffer.CustomData.bHasBackdrop then
    oTextBuffer:SetVisible(true)
  end
  return true
end

function GetMessageHeight(oTextWidget)
  return oTextWidget:GetHeight()
end

function WrapText(oTextWidget)
  oTextWidget:Wrap()
end

function IsEmpty(tTarget)
  if type(tTarget) ~= "table" then
    return nil
  end
  return table.getn(tTarget) == 0
end

function MboxAbs(nNumber)
  if 0 < nNumber then
    return nNumber
  end
  return nNumber * -1
end

function ValidateParameter(Parameter, sType, DefaultValue)
  if type(Parameter) == sType then
    return Parameter
  else
    return DefaultValue
  end
end

function HandleE3HudModeEvent(oWidget, tEvent)
  if tEvent.bOn then
    oWidget.CustomData.bE3HudMode = true
    MrxGuiBase.RemoveWidgetWithChildren(oWidget:GetChildren()[1])
    for n, oW in pairs(oWidget:GetChildren()[1].CustomData.CurrentMessages) do
      oW:SetVisible(false)
    end
  elseif oWidget.CustomData.bE3HudMode then
    oWidget.CustomData.bE3HudMode = false
    MrxGuiBase.AddWidgetWithChildren(oWidget:GetChildren()[1])
    for n, oW in pairs(oWidget:GetChildren()[1].CustomData.CurrentMessages) do
      oW:SetVisible(true)
    end
  end
end

function HandleAddMessageEvent(oWidget, tEvent)
  if tEvent.sMessage then
    oWidget:AddMessage(tEvent.sMessage, nil, tEvent.nDuration)
  end
end

function DrawDebugRectangle(TargetWidget)
  local thing = {
    CommandType = "rectangle",
    x1 = 10,
    y1 = 10,
    x2 = 400,
    y2 = 300,
    u1 = 0,
    v1 = 0,
    u2 = 1,
    v2 = 1,
    RedLevel = 128,
    GreenLevel = 0,
    BlueLevel = 0,
    TranslucencyLevel = 192,
    texture = nil,
    HorizontalAnchor = "left",
    VerticalAnchor = "top"
  }
  TargetWidget.DrawingCommands[2] = thing
end
