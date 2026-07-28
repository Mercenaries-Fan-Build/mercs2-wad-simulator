import("MrxGui")
import("MrxGuiBase")
import("MrxGuiHudMessage")
import("MrxGuiSupportShop")
import("MrxGuiTutorial")
import("MrxGuiHudFactionGauge")
import("MrxTaskObjective")
import("MrxUtil")
import("MrxBootstrap")
import("WifMissionData")
import("MrxSoundCategories")
HudInterface = {}
_G.Hud = HudInterface
PdaInterface = {}
_G.oPda = PdaInterface
_G.Pda = PdaInterface
HudInterface.Radar = {}

function HudInterface.Radar:AddObjective(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Minimap")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddObjective(tArgs.sName, tArgs.nX, tArgs.nY, tArgs.nZ, tArgs.nR, tArgs.nG, tArgs.nB, tArgs.nWidth, tArgs.nHeight, tArgs.sTexture, tArgs.uGuid, tArgs.bSticky, tArgs.bRotate, tArgs.bOriented, tArgs.nSortOrder)
  end
  if Net.IsServer() and not tArgs.bDontNetSync then
    Net.SendEvent_AddRadarObjective(tArgs.sName or "", tArgs.nX or 0, tArgs.nY or 2, tArgs.nZ or 0, tArgs.nR or 255, tArgs.nG or 255, tArgs.nB or 0, tArgs.nWidth or 3, tArgs.nHeight or 3, MrxUtil.MarkerGetIndexByName_Radar(tArgs.sTexture or ""), tArgs.uGuid or 0, tArgs.bSticky or false, tArgs.bRotate or false, tArgs.bOriented or false, tArgs.nSortOrder or 5)
  end
end

function HudInterface.Radar:UpdateObjective(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Minimap")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddObjective(tArgs.sName, tArgs.nX, tArgs.nY, tArgs.nZ, tArgs.nR, tArgs.nG, tArgs.nB, tArgs.nWidth, tArgs.nHeight, tArgs.sTexture, tArgs.uGuid, tArgs.bSticky, tArgs.bRotate, tArgs.bOriented, tArgs.nSortOrder)
  end
  if Net.IsServer() and not tArgs.bDontNetSync then
    Net.SendEvent_AddRadarObjective(tArgs.sName or "", tArgs.nX or 0, tArgs.nY or 2, tArgs.nZ or 0, tArgs.nR or 255, tArgs.nG or 255, tArgs.nB or 0, tArgs.nWidth or 3, tArgs.nHeight or 3, MrxUtil.MarkerGetIndexByName_Radar(tArgs.sTexture or ""), tArgs.uGuid or 0, tArgs.bSticky or false, tArgs.bRotate or false, tArgs.bOriented or false, tArgs.nSortOrder or 5)
  end
end

function HudInterface.Radar:RemoveObjective(tArgs)
  if not tArgs.sName then
    Debug.Printf("Cannot remove nil objective")
    return
  end
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Minimap")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:DeleteObjective(tArgs.sName)
  end
  local tPlayers = {}
  if Net.IsServer() and not tArgs.bDontNetSync then
    Net.SendEvent_RemoveRadarObjective(tArgs.sName)
  end
end

function HudInterface.Radar:AnimateObjectiveSize(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Minimap")
  tArgs.nDuration = tArgs.nDuration or 2
  tArgs.nMinWidth = tArgs.nMinWidth or 2
  tArgs.nMinHeight = tArgs.nMinHeight or 2
  tArgs.nMaxWidth = tArgs.nMaxWidth or 6
  tArgs.nMaxHeight = tArgs.nMaxHeight or 6
  tArgs.nSpeedWidth = tArgs.nSpeedWidth or 8
  tArgs.nSpeedHeight = tArgs.nSpeedHeight or 8
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AnimateObjectiveSize(tArgs.sName, tArgs.nDuration, tArgs.nMinWidth, tArgs.nMinHeight, tArgs.nMaxWidth, tArgs.nMaxHeight, tArgs.bOneWay, tArgs.nSpeedWidth, tArgs.nSpeedHeight)
  end
end

function HudInterface.Radar:AnimateObjectiveAlpha(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Minimap")
  tArgs.nDuration = tArgs.nDuration or 1
  tArgs.nMinAlpha = tArgs.nMinAlpha or 0
  tArgs.nMaxAlpha = tArgs.nMaxAlpha or 1
  tArgs.bOneWay = tArgs.bOneWay or false
  tArgs.nSpeed = tArgs.nSpeed or 0.5
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AnimateObjectiveAlpha(tArgs.sName, tArgs.nDuration, tArgs.nMinAlpha, tArgs.nMaxAlpha, tArgs.bOneWay, tArgs.nSpeed)
  end
end

function HudInterface.Radar:AnimateObjectiveSonar(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Minimap")
  tArgs.nTotalBlips = tArgs.nTotalBlips or 4
  tArgs.nVisibleBlips = tArgs.nVisibleBlips or 1
  tArgs.nMinWidth = tArgs.nMinWidth or 2
  tArgs.nMaxWidth = tArgs.nMaxWidth or 8
  tArgs.nBlipDelay = tArgs.nBlipDelay or 1
  tArgs.nAlphaAtMin = tArgs.nAlphaAtMin or 0
  tArgs.nAlphaAtMax = tArgs.nAlphaAtMax or 1
  tArgs.nGrowSpeed = tArgs.nGrowSpeed or 5
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AnimateObjectiveSonar(tArgs.sName, tArgs.nDuration, tArgs.sTexture, tArgs.nTotalBlips, tArgs.nVisibleBlips, tArgs.nMinWidth, tArgs.nMaxWidth, tArgs.nBlipDelay, tArgs.nAlphaAtMin, tArgs.nAlphaAtMax, tArgs.nGrowSpeed, tArgs.nRed, tArgs.nGreen, tArgs.nBlue)
  end
end

function HudInterface.Radar:UnanimateObjective(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Minimap")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:UnanimateObjective(tArgs.sName, tArgs.sType)
  end
end

function HudInterface.Radar:AddLineRegion(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "MinimapFlash")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddRegion(tArgs.uGuid, tArgs.nRed, tArgs.nGreen, tArgs.nBlue, tArgs.nAlpha, tArgs.bInvert)
  end
end

function HudInterface.Radar:RemoveLineRegion(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "MinimapFlash")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:RemoveRegion(tArgs.uGuid)
  end
end

HudInterface.MessageBox = {sName = "MessageBox"}

function HudInterface.MessageBox:AddMessage(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, self.sName)
  local tMessageIds = {}
  local nId
  for nIndex, oWidget in pairs(tWidgets) do
    nId = oWidget:AddMessage(tArgs.sMessage, tArgs.nPriority, tArgs.nDuration, tArgs.nFadeTime, tArgs.bClearBuffer, tArgs.bAllowsAppends, tArgs.fCallback, tArgs.tCallbackData)
    tMessageIds[oWidget:GetOwner()] = nId
  end
  return tMessageIds
end

function HudInterface.MessageBox:ModifyPendingMessage(tArgs)
  local bSuccess = true
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, self.sName)
  for nIndex, oWidget in pairs(tWidgets) do
    bSuccess = bSuccess and oWidget:ModifyPendingMessage(tArgs.tMessageIds[oWidget:GetOwner()], tArgs.sMessage, tArgs.nDuration, tArgs.nFadeTime, tArgs.bClearBuffer, tArgs.bAllowsAppends, tArgs.fCallback, tArgs.tCallbackData)
  end
  return bSuccess
end

function HudInterface.MessageBox:RemovePendingMessage(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, self.sName)
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:RemovePendingMessage(tArgs.tMessageIds[oWidget:GetOwner()])
  end
end

function HudInterface.MessageBox:Clear(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, self.sName)
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:ClearMessages()
  end
end

tObjectiveMessageConfigs = {
  add = {
    sColor = "[objt]",
    sHexColor = "FFC800",
    sPrefix = "Objective",
    sStatus = "added",
    nPriority = 1,
    sSoundCue = "ui_HUD_Objective_New"
  },
  bonus_add = {
    sColor = "[2ndobjt]",
    sHexColor = "33CC99",
    sPrefix = "Bonus",
    sStatus = "added",
    nPriority = 2,
    sSoundCue = "ui_HUD_Objective_New"
  },
  bty_add = {
    sColor = "[2ndobjt]",
    sHexColor = "33CC99",
    sPrefix = "Bounty",
    sStatus = "added",
    nPriority = 3,
    sSoundCue = "ui_HUD_Objective_New"
  },
  upd = {
    sColor = "[objt]",
    sHexColor = "FFC800",
    sPrefix = "Objective",
    sStatus = "updated",
    nPriority = 1,
    sSoundCue = "ui_HUD_Objective_Update"
  },
  bonus_upd = {
    sColor = "[2ndobjt]",
    sHexColor = "33CC99",
    sPrefix = "Bonus",
    sStatus = "updated",
    nPriority = 2,
    sSoundCue = "ui_HUD_Objective_Update"
  },
  bty_upd = {
    sColor = "[2ndobjt]",
    sHexColor = "33CC99",
    sPrefix = "Bounty",
    sStatus = "updated",
    nPriority = 3,
    sSoundCue = "ui_HUD_Objective_Update"
  },
  cpl = {
    sColor = "[objt]",
    sHexColor = "FFC800",
    sPrefix = "Objective",
    sStatus = "completed",
    nPriority = 1,
    sSoundCue = "ui_HUD_Objective_Complete"
  },
  bonus_cpl = {
    sColor = "[2ndobjt]",
    sHexColor = "33CC99",
    sPrefix = "Bonus",
    sStatus = "completed",
    nPriority = 2,
    sSoundCue = "ui_HUD_Objective_Complete"
  },
  bty_cpl = {
    sColor = "[2ndobjt]",
    sHexColor = "33CC99",
    sPrefix = "Bounty",
    sStatus = "completed",
    nPriority = 3,
    sSoundCue = "ui_HUD_Objective_Complete"
  },
  ccl = {
    sColor = "[red]",
    sHexColor = "FF0000",
    sPrefix = "Objective",
    sStatus = "cancelled",
    nPriority = 1
  },
  bonus_ccl = {
    sColor = "[red]",
    sHexColor = "FF0000",
    sPrefix = "Bonus",
    sStatus = "cancelled",
    nPriority = 2
  },
  bty_ccl = {
    sColor = "[red]",
    sHexColor = "FF0000",
    sPrefix = "Bounty",
    sStatus = "cancelled",
    nPriority = 3
  },
  collectible_upd = {
    sColor = "[2ndobjt]",
    sHexColor = "33CC99",
    sPrefix = "Collectible",
    sStatus = "updated",
    nPriority = 4,
    sSoundCue = "ui_HUD_Objective_Update"
  }
}

function GetMessageTypeFromHash(uStringHash)
  for sMsgType, tData in pairs(tObjectiveMessageConfigs) do
    if String.GetHash(sMsgType) == uStringHash then
      return sMsgType
    end
  end
end

_tMsgIdsByGroup = {}

function DisplayObjectiveMessage(bDisplay, sInlineIcon, sMsgType, sShortDesc, sGroupId, fCallback, tCallbackArgs)
  if Net.IsClient() and sInlineIcon == 99 then
    Hud.MessageBox:AddMessage({
      sMessage = "[Generic.CheckpointReached]"
    })
    return
  end
  if Net.IsClient() and type(sInlineIcon) ~= "string" then
    sInlineIcon = MrxUtil.GetInlineIconNameByIndex(sInlineIcon)
  end
  if Net.IsClient() and type(sGroupId) ~= "string" then
    sGroupId = tostring(sGroupId)
  end
  if type(sMsgType) ~= "string" then
    sMsgType = GetMessageTypeFromHash(sMsgType)
  end
  local tMessageConfig = tObjectiveMessageConfigs[sMsgType]
  if not tObjectiveMessageConfigs[sMsgType] then
    return
  end
  local sStatus = tMessageConfig.sStatus
  local sColor = tMessageConfig.sColor
  local sHexColor = tMessageConfig.sHexColor
  local sPrefix = tMessageConfig.sPrefix
  local nPriority = tMessageConfig.nPriority
  local sSoundCue = tMessageConfig.sSoundCue
  local sPrefix = sInlineIcon .. " [objective." .. sPrefix .. sStatus .. "]"
  local sMsg
  sMsg = sColor .. sPrefix .. " " .. sShortDesc
  Debug.Printf(sMsg)
  if bDisplay then
    if Net.IsServer() then
      Net.SendEvent_ObjectiveMessage(MrxUtil.GetInlineIconIndexByName(sInlineIcon), sMsgType, sShortDesc, StringToGuid(sGroupId))
    end
    if MrxBootstrap.IsGuiLoaded() ~= true then
      Event.Create(Event.TimerRelative, {1}, DisplayObjectiveMessage, {
        bDisplay,
        sInlineIcon,
        sMsgType,
        sShortDesc,
        sGroupId,
        fCallback,
        tCallbackArgs
      })
    else
      local function _MsgDisplayed()
        if sGroupId then
          _sGroupIdOfDisplayedMsg = sGroupId
          
          _tMsgIdsByGroup[sGroupId] = nil
        end
        MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
      end
      
      local bSuccess = false
      if sGroupId and _sGroupIdOfDisplayedMsg ~= sGroupId then
        local tMsgIds = _tMsgIdsByGroup[sGroupId]
        if tMsgIds then
          bSuccess = Hud.MessageBox:ModifyPendingMessage({tMessageIds = tMsgIds, sMessage = sMsg})
        end
      end
      if not bSuccess then
        if sGroupId and _sGroupIdOfDisplayedMsg == sGroupId then
          nPriority = 0
        end
        local tMsgIds = Hud.MessageBox:AddMessage({
          sMessage = sMsg,
          nPriority = nPriority,
          nDuration = 5,
          bClearBuffer = true,
          bAllowsAppends = false,
          fCallback = _MsgDisplayed
        })
        if sGroupId and tMsgIds then
          _tMsgIdsByGroup[sGroupId] = tMsgIds
        end
      end
      Pda.Database:AddLogEntry({
        sType = "objective",
        sName = "",
        sMessage = sPrefix .. " " .. sShortDesc,
        sColor = sHexColor
      })
    end
    if _evClientJoined then
      Event.Delete(_evClientJoined)
    end
    if sMsgType == "add" or sMsgType == "bonus_add" or sMsgType == "bty_add" or sMsgType == "upd" or sMsgType == "bonus_upd" or sMsgType == "bty_upd" then
      _evClientJoined = Event.CreatePersistent(Event.ScriptEvent, {
        "mpPlayerJoin",
        function(tData)
          return Net.IsServer() and not Player.IsLocal(tData[1])
        end
      }, Net.SendEvent_ObjectiveMessage, {
        MrxUtil.GetInlineIconIndexByName(sInlineIcon),
        sMsgType,
        sShortDesc,
        StringToGuid(sGroupId)
      })
    end
    if sSoundCue then
      Sound.CueSound(0, sSoundCue)
    end
  end
end

HudInterface.SupportMenu = {}

function HudInterface.SupportMenu:AddItem(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Support Menu")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddItem({
      sName = tArgs.sName,
      sIcon = tArgs.sIcon,
      oSupport = tArgs.oSupport,
      bAnimate = tArgs.bAnimate,
      bDontNetSync = tArgs.bDontNetSync
    })
  end
end

function HudInterface.SupportMenu:RemoveItem(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Support Menu")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:RemoveItem(tArgs.sName, tArgs.bDontNetSync)
  end
end

function HudInterface.SupportMenu:SetShootingGalleryMode(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Support Menu")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetShootingGalleryMode(tArgs.bEnable)
  end
end

HudInterface.ObjectiveTray = {}
tClientSlotText = {}

function HudInterface.ObjectiveTray:SetSlotToText(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Objective Tray")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetSlotToText(tArgs.nSlot, tArgs.sText)
  end
  local tPlayers = {}
  if Net.IsServer() and not tArgs.bDontNetSync then
    if "table" == type(tArgs.vPlayer) then
      tPlayers = tArgs.vPlayer
    elseif "userdata" == type(tArgs.vPlayer) then
      tPlayers = {
        tArgs.vPlayer
      }
    elseif not tArgs.vPlayer then
      tPlayers = Player.GetAllPlayers()
    else
      return
    end
    for nIndex, uGuid in pairs(tPlayers) do
      if Player.IsRemote(Player.GetPlayerId(uGuid)) then
        Net.SendEvent_SetObjectiveTraySlotText(uGuid, tArgs.nSlot, tArgs.sText)
      end
    end
    tClientSlotText[tArgs.nSlot] = tArgs.sText
    Event.Delete(_evSetClientSlotText)
    _evSetClientSlotText = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, SendSlotText)
  end
end

function SendSlotText()
  Event.Create(Event.ObjectHibernation, {
    Player.GetSecondaryCharacter(),
    "awake"
  }, function()
    for nSlot, sText in pairs(tClientSlotText) do
      Net.SendEvent_SetObjectiveTraySlotText(Player.GetSecondaryPlayer(), nSlot, sText)
    end
  end)
end

function HudInterface.ObjectiveTray:SetSlotToImage(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Objective Tray")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetSlotToImage(tArgs.nSlot, tArgs.sTexture, tArgs.nWidth, tArgs.nHeight)
  end
  local tPlayers = {}
  if Net.IsServer() and not tArgs.bDontNetSync then
    if "table" == type(tArgs.vPlayer) then
      tPlayers = tArgs.vPlayer
    elseif "userdata" == type(tArgs.vPlayer) then
      tPlayers = {
        tArgs.vPlayer
      }
    elseif not tArgs.vPlayer then
      tPlayers = Player.GetAllPlayers()
    else
      return
    end
    for nIndex, uGuid in pairs(tPlayers) do
      if Player.IsRemote(Player.GetPlayerId(uGuid)) then
        Net.SendEvent_SetObjectiveTraySlotImage(uGuid, tArgs.nSlot, tArgs.sTexture, tArgs.nWidth, tArgs.nHeight)
      end
    end
  end
end

function HudInterface.ObjectiveTray:SetSlotToWidget(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Objective Tray")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetSlotToWidget(tArgs.nSlot, tArgs.oWidget)
  end
end

function HudInterface.ObjectiveTray:ClearSlot(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Objective Tray")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:ClearSlot(tArgs.nSlot)
  end
  tClientSlotText[tArgs.nSlot] = nil
  local tPlayers = {}
  if Net.IsServer() and not tArgs.bDontNetSync then
    if "table" == type(tArgs.vPlayer) then
      tPlayers = tArgs.vPlayer
    elseif "userdata" == type(tArgs.vPlayer) then
      tPlayers = {
        tArgs.vPlayer
      }
    elseif not tArgs.vPlayer then
      tPlayers = Player.GetAllPlayers()
    else
      return
    end
    for nIndex, uGuid in pairs(tPlayers) do
      if Player.IsRemote(Player.GetPlayerId(uGuid)) then
        Net.SendEvent_ClearObjectiveTraySlot(uGuid, tArgs.nSlot)
      end
    end
  end
end

function NetClientSetObjectiveTraySlot(aSlot, aIsImage, aText, aImageHash, aWidth, aHeight)
  if not Net.IsServer() then
    local oWidget = MrxGuiBase.GetWidgetByName("Objective Tray")
    if not oWidget then
      Event.Create(Event.TimerRelative, {2}, NetClientSetObjectiveTraySlot, {
        aSlot,
        aIsImage,
        aText,
        aImageHash,
        aWidth,
        aHeight
      })
      return
    end
    if aIsImage then
      Hud.ObjectiveTray:SetSlotToImage({
        nSlot = aSlot,
        sTexture = aImageHash,
        nWidth = aWidth,
        nHeight = aHeight
      })
    else
      Hud.ObjectiveTray:SetSlotToText({nSlot = aSlot, sText = aText})
    end
  end
end

function NetClientClearObjectiveTraySlot(aSlot)
  if not Net.IsServer() then
    Hud.ObjectiveTray:ClearSlot({nSlot = aSlot})
  end
end

HudInterface.MapLabel = {}

function HudInterface.MapLabel:Show(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Map Label")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:Show(tArgs.sLocation, tArgs.nDuration, true)
  end
end

HudInterface.Announcement = {}

function HudInterface.Announcement:Show(tArgs)
  local tPlayers
  if not vPlayer then
    tPlayers = Player.GetAllPlayers()
  elseif "userdata" == type(vPlayer) then
    tPlayers = {tPlayer}
  elseif "table" == type(vPlayer) then
    tPlayers = vPlayer
  end
  for nIndex, uPlayerGuid in pairs(tPlayers) do
    MrxGuiHudMessage.ShowMessage(uPlayerGuid, tArgs.sTexture, tArgs.fZoomCallback, tArgs.fFadeCallback, tArgs.nX, tArgs.nY, tArgs.sHorizontalAnchor, tArgs.sVerticalAnchor, tArgs.nWidth, tArgs.nHeight, tArgs.nDuration, tArgs.vSoundEffect)
  end
end

_tFanfareQueue = {bPending = true, bPaused = false}

function _tFanfareQueue:Append(fAction)
  if self.bPending then
    Debug.Printf(string.format("FanfareQueue: Commit %d %s", #self, tostring(fAction)))
    self.bPending = false
    fAction(self)
  else
    Debug.Printf(string.format("FanfareQueue: Append %d %s", #self, tostring(fAction)))
    table.insert(self, fAction)
  end
end

function _tFanfareQueue:Advance()
  if not _tFanfareQueue.bPaused and not self.bAdvancing then
    if Net.IsClient() and bClientPauseFanfare then
      return
    end
    if 0 < #self then
      self.bAdvancing = true
      local fAction = self[1]
      table.remove(self, 1)
      Debug.Printf(string.format("FanfareQueue: Advance %d %s", #self, tostring(fAction)))
      fAction(self)
    else
      Debug.Printf(string.format("FanfareQueue: Pending"))
      self.bPending = true
    end
  end
end

function _tFanfareQueue:FinishItem()
  self.bAdvancing = false
  self:Advance()
end

HudInterface.FanfareQueue = {}

function HudInterface.FanfareQueue:Append(fAction, ...)
  local tActionData = {
    ...
  }
  _tFanfareQueue:Append(function(self)
    if fAction then
      fAction(unpack(tActionData))
    end
    self:FinishItem()
  end)
end

function HudInterface.FanfareQueue.Pause(bPause)
  _tFanfareQueue.bPaused = bPause
  if not bPause then
    _tFanfareQueue:Advance()
  end
end

function HudInterface.FanfareQueue.ClientSetPending(bPending)
  _tFanfareQueue.bPending = bPending
end

bClientPauseFanfare = false

function HudInterface.FanfareQueue.ClientPause(bPause)
  bClientPauseFanfare = bPause
  if not bClientPauseFanfare then
    _tFanfareQueue:Advance()
  end
end

HudInterface.JobFanfare = {}

function HudInterface.JobFanfare:Complete(tArgs)
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.ShowCompletedMessage(nil, nil, function()
      if tArgs.fCallback then
        tArgs.fCallback(tArgs.tCallbackData)
      end
      self:FinishItem()
    end)
  end)
end

function HudInterface.JobFanfare:Failed(tArgs)
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.ShowFailedMessage(nil, nil, function()
      if tArgs.fCallback then
        tArgs.fCallback(tArgs.tCallbackData)
      end
      self:FinishItem()
    end)
  end)
end

HudInterface.Fanfare = {}

function HudInterface.Fanfare:Create(tArgs)
  local sType = "contract"
  if "wager" == tArgs.sType then
    sType = "wager"
  elseif "mission" == tArgs.sType then
    sType = "mission"
  end
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.CreateFanfare(sType)
    MrxGuiHudMessage.SetFanfareParameters(tArgs.sProfileName1, tArgs.sProfileName2, tArgs.sCancelMsg, tArgs.bAllowRetry)
    MrxGuiHudMessage.SetFanfareCompleteCallback(function(...)
      if tArgs.fCallback then
        tArgs.fCallback(...)
      end
      self:FinishItem()
    end, tArgs.tCallbackData)
    self:FinishItem()
  end)
  return true
end

function HudInterface.Fanfare:AddItem(tArgs)
  local sDescription = tArgs.sDescription
  local nValue = tArgs.nValue
  local sValueType = tArgs.sValueType
  local nPlayer = tArgs.nPlayer
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.AddFanfareLineItem(sDescription, nValue, sValueType, nPlayer)
    self:FinishItem()
  end)
  return true
end

function Hud.Fanfare:Commence(tArgs)
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.CommenceFanfare(tArgs.nSlowdownDuration)
  end)
  return true
end

HudInterface.SupportFanfare = {}

function Hud.SupportFanfare:Create(tArgs)
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.CreateFanfare("support")
    MrxGuiHudMessage.SetFanfareCompleteCallback(function(...)
      if tArgs.fCallback then
        tArgs.fCallback(...)
      end
      self:FinishItem()
    end, tArgs.tCallbackData)
    self:FinishItem()
  end)
  return true
end

function Hud.SupportFanfare:AddItem(tArgs)
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.SupportFanfareAddItem(tArgs.sTexture, tArgs.sItemName, tArgs.sFaction, tArgs.sContactName, tArgs.sBlipName)
    self:FinishItem()
  end)
  return true
end

function Hud.SupportFanfare:Commence(tArgs)
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.SupportFanfareCommence()
  end)
  return true
end

HudInterface.ContactFanfare = {}

function Hud.ContactFanfare:Commence(tArgs)
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.CreateFanfare("contact")
    MrxGuiHudMessage.SetFanfareCompleteCallback(function(...)
      if tArgs.fCallback then
        tArgs.fCallback(...)
      end
      self:FinishItem()
    end, tArgs.tCallbackData)
    MrxGuiHudMessage.ContactFanfareCommence("s", "s", "s")
  end)
  return true
end

HudInterface.CardFanfare = {}

function Hud.CardFanfare:Commence(tArgs)
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.CreateFanfare("card", tArgs.sFaction)
    MrxGuiHudMessage.CardFanfareSetParameters(tArgs.sTitle, tArgs.sName, tArgs.sJobTitle, tArgs.sPhone1, tArgs.sPhone2, tArgs.sEmail, tArgs.nDisplayTime)
    MrxGuiHudMessage.SetFanfareCompleteCallback(function(...)
      if tArgs.fCallback then
        tArgs.fCallback(...)
      end
      self:FinishItem()
    end, tArgs.tCallbackData)
    MrxGuiHudMessage.CardFanfareCommence()
  end)
end

Hud.TextFanfare = {}

function Hud.TextFanfare:Commence(tArgs)
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.ShowTextFanfare(nil, tArgs.sLine1, tArgs.sLine2, tArgs.nEntranceTime, tArgs.nDisplayTime, tArgs.nFadeTime, function(...)
      Debug.Printf(string.format("TextFanfare %s %s Callback %s(%s)", tArgs.sLine1, tArgs.sLine2, tostring(tArgs.fCallback), tostring(...)))
      if tArgs.fCallback then
        tArgs.fCallback(...)
      end
      self:FinishItem()
    end, tArgs.tCallbackData)
  end)
end

HudInterface.EventFanfare = {}

function Hud.EventFanfare:Commence(tArgs)
  if not tArgs.vText and tArgs.sText then
    tArgs.vText = tArgs.sText
  end
  _tFanfareQueue:Append(function(self)
    MrxGuiHudMessage.ShowEventFanfare(tArgs.sType, tArgs.vText, function(...)
      if tArgs.fCallback then
        tArgs.fCallback(...)
      end
      self:FinishItem()
    end, tArgs.tCallbackData)
    if "string" == type(tArgs.vText) then
      Pda.Database:AddLogEntry({
        sType = "event",
        sName = "",
        sMessage = MrxGuiHudMessage.GetEventFanfareTitle(tArgs.sType) .. ": " .. tArgs.vText,
        sColor = "3399FF"
      })
    elseif "table" == type(tArgs.vText) then
      for n, s in ipairs(tArgs.vText) do
        Pda.Database:AddLogEntry({
          sType = "event",
          sName = "",
          sMessage = MrxGuiHudMessage.GetEventFanfareTitle(tArgs.sType) .. ": " .. s,
          sColor = "3399FF"
        })
      end
    end
  end)
end

HudInterface.Cinematic = {}

function HudInterface.Cinematic:Show(tArgs)
  local oWidget = MrxGuiBase.GetWidgetByName("Cinematic Placeholder")
  oWidget:ShowMovie(tArgs.sMovie, tArgs.nFadeInTime, tArgs.nFadeOutTime, tArgs.fCallback, tArgs.tCallbackData, tArgs.bSubtitles)
end

function HudInterface.Cinematic:Hide()
  local oWidget = MrxGuiBase.GetWidgetByName("Cinematic Placeholder")
  oWidget:Hide()
end

function HudInterface.Cinematic:Play()
  local oWidget = MrxGuiBase.GetWidgetByName("Cinematic Placeholder")
  oWidget:Play()
end

function HudInterface.Cinematic:Pause()
  local oWidget = MrxGuiBase.GetWidgetByName("Cinematic Placeholder")
  oWidget:Pause()
end

function NetClientShowMovie(sMovieName, nFadeIn, nFadeOut, bSubtitlesFlag)
  if not Net.IsServer() then
    MrxSoundCategories.DuckMasterVolume(0.5)
    Hud.Cinematic:Show({
      sMovie = sMovieName,
      nFadeInTime = nFadeIn,
      nFadeOutTime = nFadeOut,
      bSubtitles = bSubtitlesFlag
    })
  end
end

function NetClientHideMovie()
  if not Net.IsServer() then
    MrxSoundCategories.UnduckMasterVolume(0.5)
    local oWidget = MrxGuiBase.GetWidgetByName("Cinematic Placeholder")
    oWidget:HideSlow()
  end
end

function NetClientIsMovieRunning()
  if not Net.IsServer() then
    local oWidget = MrxGuiBase.GetWidgetByName("Cinematic Placeholder")
    if oWidget then
      return oWidget:IsMovieRunning()
    else
      return false
    end
  end
end

function NetClientIsMovieHiding()
  if not Net.IsServer() then
    local oWidget = MrxGuiBase.GetWidgetByName("Cinematic Placeholder")
    if oWidget then
      return oWidget:IsMovieHiding()
    else
      return false
    end
  end
end

HudInterface.CinematicPlaceholder = {}

function HudInterface.CinematicPlaceholder:Show(tArgs)
  local oWidget = MrxGuiBase.GetWidgetByName("Cinematic Placeholder")
  oWidget:Show(tArgs.sTexture, tArgs.sCaption, tArgs.nFadeInTime, tArgs.nFadeOutTime, tArgs.fCallback, tArgs.tCallbackData)
end

function HudInterface.CinematicPlaceholder:Hide(tArgs)
  local oWidget = MrxGuiBase.GetWidgetByName("Cinematic Placeholder")
  oWidget:Hide()
end

HudInterface.FactionDisplay = {}

function HudInterface.FactionDisplay:Show(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Faction Display")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:ShowAll(tArgs.nDuration)
  end
end

function HudInterface.FactionDisplay:SetValue(tArgs)
  if Net.IsClient() and not tArgs.bForceOnClient then
    return
  end
  if Net.IsServer() then
    Net.SendEvent_PursuitMessage(0, tArgs.sFaction, tArgs.nValue)
  end
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Faction Display")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetValue(tArgs.sFaction, tArgs.nValue, tArgs.bInitialize)
  end
end

function HudInterface.FactionDisplay:SetInsideFactionZone(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Faction Display")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetInsideFactionZone(tArgs.sFaction, tArgs.bInside, tArgs.bInitialize)
  end
end

function HudInterface.FactionDisplay:ConfigureThresholds(tArgs)
  MrxGuiHudFactionGauge.SetLevels(tArgs.tLevelThresholds, tArgs.tLevelNames, tArgs.sPursuitName, tArgs.bDisplayResult)
end

function HudInterface.FactionDisplay:AddMeter(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Faction Display")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddFactionGauge(tArgs.sFaction, tArgs.sTexture)
  end
end

function HudInterface.FactionDisplay:StartTimer(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Faction Display")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:StartTimer(tArgs.sFaction, tArgs.nDuration, tArgs.fCallback, tArgs.tCallbackData)
  end
end

function HudInterface.FactionDisplay:StartPursuit(tArgs)
  if Net.IsClient() and not tArgs.bForceOnClient then
    return
  end
  if Net.IsServer() then
    Net.SendEvent_PursuitMessage(1, tArgs.sFaction, tArgs.nDuration)
  end
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Faction Display")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:StartPursuit(tArgs.sFaction, tArgs.nDuration, tArgs.fCallback, tArgs.tCallbackData)
  end
end

function HudInterface.FactionDisplay:HideMeter(tArgs)
  if Net.IsClient() and not tArgs.bForceOnClient then
    return
  end
  if Net.IsServer() then
    Net.SendEvent_PursuitMessage(2, tArgs.sFaction)
  end
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Faction Display")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:HideGauge(tArgs.sFaction)
  end
end

function HudInterface.FactionDisplay:RemoveMeter(tArgs)
end

function HudInterface.FactionDisplay:RemoveAllMeters(tArgs)
end

HudInterface.SubtitleBuffer = {
  sName = "Subtitle Buffer"
}
HudInterface.SubtitleBuffer.AddMessage = HudInterface.MessageBox.AddMessage
HudInterface.SubtitleBuffer.ModifyPendingMessage = HudInterface.MessageBox.ModifyPendingMessage
HudInterface.SubtitleBuffer.RemovePendingMessage = HudInterface.MessageBox.RemovePendingMessage
HudInterface.SubtitleBuffer.Clear = HudInterface.MessageBox.Clear
HudInterface.Shop = {}

function HudInterface.Shop:Create(tArgs)
  return MrxGuiSupportShop.Create(tArgs.uPlayer)
end

function HudInterface.Shop:AddItem(tArgs)
  return MrxGuiSupportShop.AddItem(tArgs.uPlayer, tArgs.sName, tArgs.nCashCost, tArgs.nCurrentStock, tArgs.nMaxStock, tArgs.bUnlocked, tArgs.sId or tArgs.sName, tArgs.bFuelTank, tArgs.nFuelQuantity, tArgs.sRawName)
end

function HudInterface.Shop:AddItemFull(tArgs)
  return MrxGuiSupportShop.AddItemFull(tArgs.uPlayer, tArgs.sName, tArgs.sDescription, tArgs.sTexture, tArgs.nCashCost, tArgs.nCurrentStock, tArgs.nMaxStock, tArgs.bUnlocked, tArgs.sId or tArgs.sName, tArgs.bFuelTank, tArgs.bMarkAsNew, tArgs.nFuelQuantity, tArgs.sRawName)
end

function Hud.Shop:SetCallback(tArgs)
  return MrxGuiSupportShop.SetCallback(tArgs.uPlayer, tArgs.fCallback, tArgs.tCallbackData)
end

function Hud.Shop:SetCloseCallback(tArgs)
  return MrxGuiSupportShop.SetCloseCallback(tArgs.uPlayer, tArgs.fCallback, tArgs.tCallbackData)
end

function Hud.Shop:Commence(tArgs)
  return MrxGuiSupportShop.Commence(tArgs.uPlayer)
end

function Hud.Shop:Close(tArgs)
  Debug.Printf("Hud.Shop.Close() called")
  return MrxGuiSupportShop.Close(tArgs.uPlayer)
end

HudInterface.ResourceCounter = {}

function HudInterface.ResourceCounter:SetCash(tArgs)
  local tWidgets = _GetWidgetsForPlayers(nil, "money")
  local nValue, sReason, nIncrement
  if "number" == type(tArgs) then
    nValue = tArgs
  else
    nValue = tArgs.nValue
    sReason = tArgs.sReason
    nIncrement = tArgs.nIncrement
  end
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetValue(nValue, sReason, nIncrement)
    oWidget:Show()
  end
end

function HudInterface.ResourceCounter:SetFuel(tArgs)
  local tWidgets = _GetWidgetsForPlayers(nil, "fuel")
  local nValue, sAppend, nIncrement
  if "number" == type(tArgs) then
    nValue = tArgs
  else
    nValue = tArgs.nValue
    sAppend = tonumber(tArgs.nMax)
    nIncrement = tArgs.nIncrement
  end
  sAppend = sAppend and "/" .. sAppend
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetValue(nValue, nil, nIncrement)
    if sAppend then
      oWidget:SetAppendedString(sAppend)
    end
    oWidget:Show()
  end
end

function HudInterface.ResourceCounter:SetSuppressed(tArgs)
  local tWidgets = _GetWidgetsForPlayers(nil, "money")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetSuppressed(tArgs.bSuppressCash)
  end
  local tWidgets = _GetWidgetsForPlayers(nil, "fuel")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetSuppressed(tArgs.bSuppressFuel)
  end
end

function HudInterface.ResourceCounter:Show(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "money")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:Show(tArgs.nDuration or 3)
  end
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "fuel")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:Show(tArgs.nDuration or 3)
  end
end

function HudInterface.ResourceCounter:Hide(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "money")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:Hide()
  end
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "fuel")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:Hide()
  end
end

HudInterface.Tutorial = {}

function HudInterface.Tutorial:SetText(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "tutorial")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetText(tArgs.sText)
  end
end

function HudInterface.Tutorial:ShowTutorialForObject(tArgs)
  local tPlayers
  if "table" == type(tArgs.vPlayer) then
    tPlayers = vPlayer
  elseif "userdata" == type(tArgs.vPlayer) then
    tPlayers = {}
  else
    tPlayers = Player.GetAllPlayers()
  end
  for nIndex, uPlayer in pairs(tPlayers) do
    MrxGuiTutorial.DisplayTutorialForObject(uPlayer, tArgs.sMessage, tArgs.uGuid, tArgs.fCallback, tArgs.tCallbackData)
  end
end

function HudInterface.Tutorial:ShowTutorialOnscreen(tArgs)
  local tPlayers
  if "table" == type(tArgs.vPlayer) then
    tPlayers = vPlayer
  elseif "userdata" == type(tArgs.vPlayer) then
    tPlayers = {}
  else
    tPlayers = Player.GetAllPlayers()
  end
  for nIndex, uPlayer in pairs(tPlayers) do
    MrxGuiTutorial.DisplayTutorial(uPlayer, tArgs.sMessage, tArgs.nX1, tArgs.nY1, tArgs.nX2, tArgs.nY2, tArgs.sHorizAnchor, tArgs.sVertAnchor, tArgs.fCallback, tArgs.tCallbackData)
  end
end

Hud.ClassyText = {}

function Hud.ClassyText:ShowText(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "ClassyText")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:ShowText(tArgs.sText, nil, tArgs.nY, tArgs.nDuration, nil, tArgs.sJustification, tArgs.sVertAnchor, tArgs.sJustification, tArgs.bExpand)
  end
end

HudInterface.Satellite = {}

function HudInterface.Satellite:SetTutorialText(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "Satellite overlay")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetHelpText(tArgs.sText or " ")
  end
end

function PdaInterface:SetSuppressed(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetSuppressed(tArgs.bSuppress)
  end
end

PdaInterface.Map = {}

function PdaInterface.Map:AddBlip(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddMapBlip(tArgs.sName, tArgs.nX, tArgs.nY, tArgs.sLabel, tArgs.sDesc, tArgs.uGuid, tArgs.sTexture, tArgs.sMission, tArgs.nMeter, tArgs.bSticky, tArgs.bTodoList, tArgs.sFaction or "PMC", tArgs.nSortOrder or 5)
  end
  if Net.IsServer() and not tArgs.bDontNetSync then
    local iSticky = 0
    if tArgs.bSticky ~= nil then
      if tArgs.bSticky then
        iSticky = 1
      else
        iSticky = 2
      end
    end
    local nMarkerIndex
    if tArgs.sTexture then
      nMarkerIndex = MrxUtil.MarkerGetIndexByName_Pda(tArgs.sTexture)
    end
    Net.SendEvent_AddPdaObjective(tArgs.sName, tArgs.uGuid, tArgs.sLabel, nMarkerIndex, WifMissionData.GetMissionIndexFromId(tArgs.sMission), iSticky, tArgs.nSortOrder)
  end
end

function PdaInterface.Map:RemoveBlip(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:RemoveMapBlip(tArgs.sName)
  end
  if Net.IsServer() and not tArgs.bDontNetSync then
    Net.SendEvent_RemovePdaObjective(tArgs.sName)
  end
end

function PdaInterface.Map:AddMission(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddMapMission(tArgs.sName, tArgs.sLabel, tArgs.sDesc, tArgs.sFaction, tArgs.sDefaultBlipTexture, tArgs.sDefaultBlipLabel, tArgs.bSuppress, tArgs.bTrackable, tArgs.nSortOrder)
  end
end

function PdaInterface.Map:RemoveMission(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:RemoveMapMission(tArgs.sName)
  end
end

function PdaInterface.Map:UpdateMission(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:UpdateMapMission(tArgs.sName, tArgs.sLabel, tArgs.sDesc, tArgs.sFaction, tArgs.sDefaultBlipTexture, tArgs.sDefaultBlipLabel, tArgs.bSuppress, tArgs.bTrackable)
  end
end

function PdaInterface.Map:SetSelectedMission(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetSelectedMission(tArgs.sName, tArgs.bForceOnClient)
  end
end

function PdaInterface.Map:GetSelectedMission()
  local tWidgets = _GetWidgetsForPlayers(Player.GetLocalPlayer(), "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    return oWidget:GetSelectedMission()
  end
  return nil
end

function PdaInterface.Map:AddLineRegion(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddLineRegion(tArgs.uGuid, tArgs.nRed, tArgs.nGreen, tArgs.nBlue, tArgs.nAlpha, tArgs.bInvert)
  end
end

function PdaInterface.Map:RemoveLineRegion(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:RemoveLineRegion(tArgs.uGuid)
  end
end

function PdaInterface.Map:SetMissionTrackable(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetMissionTrackable(tArgs.sName, tArgs.bTrackable)
  end
end

function PdaInterface.Map:SetMissionTrackCallback(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetMissionTrackCallback(tArgs.fCallback, tArgs.tCallbackData)
  end
end

function PdaInterface.Map:SetMissionChangeAllowed(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetMissionChangeAllowed(tArgs.bAllow)
  end
end

function PdaInterface.Map:SetFakePlayerLocation(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetFakePlayerLocation(tArgs.nX, tArgs.nY, tArgs.nZ)
  end
end

function PdaInterface.Map:SetBeaconTutorialMode(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetBeaconTutorialMode(tArgs.bEnable)
  end
end

PdaInterface.Support = {}

function PdaInterface.Support:AddItem(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddSupport(tArgs)
  end
end

function PdaInterface.Support:RemoveItem(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:RemoveSupport(tArgs.sName)
  end
end

function PdaInterface.Support:UpdateItem(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:UpdateSupport(tArgs.sName, tArgs.sDescription, tArgs.sIcon, tArgs.nStock, tArgs.nMaxStock, tArgs.nFuelCost, tArgs.oSupport, tArgs.sType)
  end
end

function PdaInterface.Support:SetEquippedItem(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetEquippedSupport(tArgs.sName, tArgs.sId)
  end
end

function PdaInterface.Support:ReadEquippedSupport(tArgs)
  local oPda = MrxGui.GetWidgetByNameAndOwner("PDA", tArgs.uPlayer)
  if oPda then
    return oPda:ReadEquippedSupport()
  end
end

function PdaInterface.Support:RestoreEquippedSupport(tArgs)
  local oPda = MrxGui.GetWidgetByNameAndOwner("PDA", tArgs.uPlayer)
  if oPda then
    oPda:RestoreEquippedSupport(tArgs.vSupport)
  end
end

PdaInterface.Database = {}

function PdaInterface.Database:SetFactionAttitude(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:SetFactionAttitude(tArgs.sName, tArgs.sTexture, tArgs.nAttitude)
  end
end

function PdaInterface.Database:AddLogEntry(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddLogEntry(tArgs.sType, tArgs.sName, tArgs.sMessage, tArgs.sColor)
  end
end

function PdaInterface.Database:AddHelpEntry(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddHelpEntry(tArgs.sTitle, tArgs.sText, tArgs.sIcon)
  end
end

function PdaInterface.Database:AddDossierEntry(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddDossierEntry(tArgs.sTitle, tArgs.sText, tArgs.sIcon)
  end
end

function PdaInterface.Database:AddStatisticCategory(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddStatisticCategory(tArgs.sCategory, tArgs.sIcon)
  end
end

function PdaInterface.Database:AddStatisticEntry(tArgs)
  local tWidgets = _GetWidgetsForPlayers(tArgs.vPlayer, "PDA")
  for nIndex, oWidget in pairs(tWidgets) do
    oWidget:AddStatisticEntry(tArgs.sCategory, tArgs.sDesc, tArgs.sData)
  end
end

PdaInterface.SubtitleBuffer = {
  sName = "PDA Subtitle Buffer"
}
PdaInterface.SubtitleBuffer.AddMessage = HudInterface.MessageBox.AddMessage
PdaInterface.SubtitleBuffer.ModifyPendingMessage = HudInterface.MessageBox.ModifyPendingMessage
PdaInterface.SubtitleBuffer.RemovePendingMessage = HudInterface.MessageBox.RemovePendingMessage
PdaInterface.SubtitleBuffer.Clear = HudInterface.MessageBox.Clear

function AddObjectiveToLocalPlayer(name, x, y, z, r, g, b, width, height, texture, uGuid, bSticky, bRotate, bOriented, nSortOrder)
  local oWidget = MrxGuiBase.GetWidgetByNameAndOwner("Minimap", Player.GetLocalPlayer())
  if oWidget then
    oWidget:AddObjective(name, x, y, z, r, g, b, width, height, MrxUtil.MarkerGetNameByIndex_Radar(texture), uGuid, bSticky or false, bRotate or false, bOriented or false, nSortOrder or 5)
  else
    Debug.Printf("NO MINIMAP YET!!!!!")
  end
end

function AddPdaBlipToLocalPlayer(sName, nX, nY, sLabel, uGuid, nTexture, nMissionIndex, bSticky, nSortOrder)
  local oWidget = MrxGuiBase.GetWidgetByNameAndOwner("PDA", Player.GetLocalPlayer())
  if oWidget then
    oWidget:AddMapBlip(sName, nX, nY, sLabel, nil, uGuid, MrxUtil.MarkerGetNameByIndex_Pda(nTexture), WifMissionData.GetMissionIdFromIndex(nMissionIndex), nil, bSticky, nil, nil, nSortOrder)
  else
    Debug.Printf("NO PDA YET!!!!!")
  end
end

function DeletePdaBlipForLocalPlayer(sName)
  local oWidget = MrxGuiBase.GetWidgetByNameAndOwner("PDA", Player.GetLocalPlayer())
  if oWidget then
    oWidget:RemoveMapBlip(sName)
  else
    Debug.Printf("NO PDA YET!!!!!")
  end
end

function DeleteObjectiveForLocalPlayer(name)
  local oWidget = MrxGuiBase.GetWidgetByNameAndOwner("Minimap", Player.GetLocalPlayer())
  if oWidget then
    oWidget:DeleteObjective(name)
  else
    Debug.Printf("NO MINIMAP YET!!!!!")
  end
end

oTestFlash = false

function TestFlash(sFile)
  if not oTestFlash then
    oTestFlash = MrxGui.FlashWidget:new()
    oTestFlash:SetFullscreen(true)
    oTestFlash:SetOwner(Player.GetLocalPlayer())
    MrxGui.AddWidget(oTestFlash)
    MrxGuiBase.GetControlFocus(oTestFlash, true)
  end
  if "string" ~= type(sFile) then
    EndFlashTest()
  else
    oTestFlash:SetSwfFile(sFile)
  end
end

function EndFlashTest()
  if oTestFlash then
    oTestFlash:SetSwfFile(nil)
    MrxGui.RemoveWidget(oTestFlash)
    MrxGuiBase.ReleaseControlFocus(oTestFlash)
    oTestFlash:delete()
    oTestFlash = false
  end
end

function Init()
  _G.TestFlash = TestFlash
  _G.EndFlashTest = EndFlashTest
end

function _GetWidgetsForPlayers(vPlayers, sName)
  if not vPlayers then
    local tWidgets = MrxGuiBase.GetAllWidgetsByName(sName)
    local tCulledWidgets = {}
    for nIndex, oWidget in pairs(tWidgets) do
      if oWidget:GetOwner() then
        table.insert(tCulledWidgets, oWidget)
      end
    end
    return tCulledWidgets
  end
  local tPlayers
  if "table" == type(vPlayers) then
    tPlayers = vPlayers
  elseif "userdata" == type(vPlayers) then
    tPlayers = {vPlayers}
  else
    return {}
  end
  local tWidgets = {}
  for nIndex, uGuid in pairs(tPlayers) do
    local oWidget = MrxGuiBase.GetWidgetByNameAndOwner(sName, uGuid)
    if oWidget then
      table.insert(tWidgets, oWidget)
    end
  end
  return tWidgets
end

function NetClientAddBoundary(uBoundary, bInclusion)
  Hud.Radar:AddLineRegion({
    uGuid = uBoundary,
    bInvert = bInclusion,
    nRed = 0,
    nGreen = 0,
    nBlue = 0,
    nAlpha = 160
  })
  Pda.Map:AddLineRegion({
    uGuid = uBoundary,
    bInvert = bInclusion,
    nRed = 0,
    nGreen = 0,
    nBlue = 0,
    nAlpha = 160
  })
end

function NetClientRemoveBoundary(uBoundary)
  Hud.Radar:RemoveLineRegion({uGuid = uBoundary})
  Pda.Map:RemoveLineRegion({uGuid = uBoundary})
end

function NetClientFactionSetValue(sFactionName, nLevel)
  Debug.Printf("NetClientFactionSetValue " .. sFactionName .. " " .. tostring(nLevel))
  Hud.FactionDisplay:SetValue({
    sFaction = sFactionName,
    nValue = nLevel,
    bForceOnClient = true
  })
end

function NetClientFactionStartPursuit(sFactionName, nTime)
  Debug.Printf("NetClientFactionStartPursuit " .. sFactionName .. " " .. tostring(nTime))
  Hud.FactionDisplay:StartPursuit({
    sFaction = sFactionName,
    nDuration = nTime,
    bForceOnClient = true
  })
end

function NetClientFactionHideMeter(sFactionName, nDummy)
  Debug.Printf("NetClientFactionHideMeter " .. sFactionName)
  Hud.FactionDisplay:HideMeter({sFaction = sFactionName, bForceOnClient = true})
end
