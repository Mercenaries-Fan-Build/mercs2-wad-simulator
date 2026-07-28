import("MrxGui")
import("MrxGuiBase")
import("MrxGuiManager")
import("MrxHqManager")
nGlobalWidth = 256
nGlobalHeight = 128
nX = 320
nY = 340
nMagnification = 5
nAnimationTimeLength = 0.1
nLifeTime = 3
nFadeTime = 1
_gFanfareFlashWidget = false
_gFullscreenFadeWidget = false

function CreateFanfare(sType, sFaction)
  if _gFanfareFlashWidget then
    return false
  end
  local sTest
  sType = sType or "contract"
  local sFile = "fanfare_contract"
  local fLoadCompleteCallback = _LoadCompleteCallback
  if "mission" == sType then
    sFile = "fanfare_mission"
  elseif "wager" == sType then
    sFile = "fanfare_wager"
  elseif "support" == sType then
    sFile = "fanfare_support_unlocked"
    fLoadCompleteCallback = _SupportLoadCompleteCallback
  elseif "contact" == sType then
    sFile = "fanfare_new_contact"
    fLoadCompleteCallback = _ContactLoadCompleteCallback
  elseif "card" == sType and "string" == type(sFaction) then
    sTest = string.lower(sFaction)
    if "an" == sTest or "ch" == sTest or "oc" == sTest or "gr" == sTest or "pr" == sTest then
      sFile = "fanfare_new_contact_" .. sFaction .. "_businesscard"
      fLoadCompleteCallback = _CardLoadCompleteCallback
    else
      return false
    end
  end
  _gFanfareFlashWidget = MrxGui.FlashWidget:new()
  _gFanfareFlashWidget:SetFullscreen(true)
  _gFanfareFlashWidget.CustomData.sFaction = sTest
  local uGlobalViewportId = _GuiInternal.GetWidgetViewport(_gFanfareFlashWidget.BasicData.uId)
  _gFanfareFlashWidget:SetOwner(Player.GetLocalPlayer())
  _gFanfareFlashWidget.CustomData.HandleFlashInput = _gFanfareFlashWidget.EventHandlers.ControllerInput
  _gFanfareFlashWidget:SetEventHandler("ControllerInput", _HandleFanfareInput)
  _GuiInternal.SetWidgetViewport(_gFanfareFlashWidget.BasicData.uId, uGlobalViewportId)
  _gFanfareFlashWidget.CustomData.bLoadingComplete = false
  _gFanfareFlashWidget.CustomData.bReadyToStart = false
  _gFanfareFlashWidget:SetVisible(false)
  _gFanfareFlashWidget:SetSwfFile(sFile, fLoadCompleteCallback, {_gFanfareFlashWidget})
  _gFanfareFlashWidget:Pause()
  _gFanfareFlashWidget.CustomData.sType = sType
  MrxGui.AddWidget(_gFanfareFlashWidget)
  if "contract" == sType or "mission" == sType or "wager" == sType then
    _gFullscreenFadeWidget = MrxGui.ImageWidget:new()
    _gFullscreenFadeWidget:SetColor(0, 0, 0, 0)
    _gFullscreenFadeWidget:SetFullscreen(true)
    _gFullscreenFadeWidget.CustomData.nAlpha = 0
  end
  return true
end

function SetFanfareCompleteCallback(fCallback, tCallbackData)
  if not _gFanfareFlashWidget then
    return false
  end
  if nil == fCallback then
    _gFanfareFlashWidget.CustomData.fCallback = nil
    _gFanfareFlashWidget.CustomData.tCallbackData = {}
    return true
  end
  if "function" ~= type(fCallback) then
    return false
  end
  local tData = tCallbackData
  if "table" ~= type(tData) then
    tData = {}
  end
  _gFanfareFlashWidget.CustomData.fCallback = fCallback
  _gFanfareFlashWidget.CustomData.tCallbackData = tData
  return true
end

function SupportFanfareAddItem(sTexture, sItemName, sFaction, sContactName, sBlipName)
  if not _gFanfareFlashWidget then
    return false
  end
  if "support" ~= _gFanfareFlashWidget.CustomData.sType then
    return false
  end
  if "string" ~= type(sTexture) then
    return false
  end
  if "string" ~= type(sItemName) then
    return false
  end
  if "string" ~= type(sFaction) then
    return false
  end
  if "string" ~= type(sContactName) then
    return false
  end
  if not _gFanfareFlashWidget.CustomData.tItemList then
    _gFanfareFlashWidget.CustomData.tItemList = {}
  end
  local tItem = {
    sTexture = sTexture,
    sItemName = sItemName,
    sFaction = sFaction,
    sContactName = sContactName,
    sBlipName = sBlipName
  }
  table.insert(_gFanfareFlashWidget.CustomData.tItemList, tItem)
  return true
end

function SupportFanfareCommence()
  if not _gFanfareFlashWidget then
    return false
  end
  if "support" ~= _gFanfareFlashWidget.CustomData.sType then
    return false
  end
  _gFanfareFlashWidget.CustomData.bReadyToStart = true
  if _gFanfareFlashWidget.CustomData.bLoadingComplete then
    _BeginSupportFanfare(_gFanfareFlashWidget)
  end
  return true
end

function _SupportLoadCompleteCallback(oWidget)
  oWidget.CustomData.bLoadingComplete = true
  if oWidget.CustomData.bReadyToStart then
    _BeginSupportFanfare(oWidget)
  end
end

function _BeginSupportFanfare(oWidget)
  oWidget:SetVisible(true)
  oWidget:Play()
  Event.Create(Event.TimerRelative, {3}, _ScrollOutFanfare, {oWidget})
end

function _ScrollOutFanfare(oWidget)
  oWidget:CallActionScriptCallback("Continue", {})
  Event.Create(Event.TimerRelative, {1}, _DeleteFanfareWidget, {oWidget})
end

function ContactFanfareCommence(sTexture, sContactName, sFaction)
  if not _gFanfareFlashWidget then
    return false
  end
  if "contact" ~= _gFanfareFlashWidget.CustomData.sType then
    return false
  end
  if "string" ~= type(sTexture) then
    return false
  end
  if "string" ~= type(sContactName) then
    return false
  end
  if "string" ~= type(sFaction) then
    return false
  end
  _gFanfareFlashWidget.CustomData.sTexture = sTexture
  _gFanfareFlashWidget.CustomData.sContactName = sContactName
  _gFanfareFlashWidget.CustomData.sFaction = sFaction
  _gFanfareFlashWidget.CustomData.bReadyToStart = true
  if _gFanfareFlashWidget.CustomData.bLoadingComplete then
    _BeginContactFanfare(_gFanfareFlashWidget)
  end
  return true
end

function _ContactLoadCompleteCallback(oWidget)
  oWidget.CustomData.bLoadingComplete = true
  if oWidget.CustomData.bReadyToStart then
    _BeginContactFanfare(oWidget)
  end
end

function _BeginContactFanfare(oWidget)
  oWidget:SetVisible(true)
  oWidget:Play()
  Event.Create(Event.TimerRelative, {3}, _ScrollOutFanfare, {oWidget})
end

function CardFanfareSetParameters(sTitle, sName, sJobTitle, sPhone1, sPhone2, sEmail, nDisplayTime)
  local oWidget = _gFanfareFlashWidget
  if not _gFanfareFlashWidget then
    return false
  end
  oWidget.CustomData.sTitle = sTitle
  oWidget.CustomData.sName = sName
  oWidget.CustomData.sJobTitle = sJobTitle
  oWidget.CustomData.sPhone1 = sPhone1
  oWidget.CustomData.sPhone2 = sPhone2
  oWidget.CustomData.sEmail = sEmail
  if "number" == type(nDisplayTime) then
    oWidget.CustomData.nDisplayTime = nDisplayTime
  else
    oWidget.CustomData.nDisplayTime = 3
  end
  if sTitle and sName and sJobTitle and sPhone1 and sPhone2 and sEmail then
    oWidget.CustomData.bCardReady = true
  end
  return true
end

function CardFanfareCommence()
  if not _gFanfareFlashWidget then
    return false
  end
  if "card" ~= _gFanfareFlashWidget.CustomData.sType then
    return false
  end
  if not _gFanfareFlashWidget.CustomData.bCardReady then
    return false
  end
  _gFanfareFlashWidget.CustomData.bReadyToStart = true
  if _gFanfareFlashWidget.CustomData.bLoadingComplete then
    _BeginCardFanfare(_gFanfareFlashWidget)
  end
  if Net.IsServer() then
    Net.SendEvent_CardFanfare(_gFanfareFlashWidget.CustomData.sFaction, _gFanfareFlashWidget.CustomData.sTitle, _gFanfareFlashWidget.CustomData.sName, _gFanfareFlashWidget.CustomData.sJobTitle, _gFanfareFlashWidget.CustomData.sPhone1, _gFanfareFlashWidget.CustomData.sPhone2, _gFanfareFlashWidget.CustomData.sEmail, _gFanfareFlashWidget.CustomData.nDisplayTime)
  end
  return true
end

function _CardLoadCompleteCallback(oWidget)
  oWidget.CustomData.bLoadingComplete = true
  if oWidget.CustomData.bReadyToStart then
    _BeginCardFanfare(oWidget)
  end
end

function _BeginCardFanfare(oWidget)
  oWidget:SetVisible(true)
  oWidget:Play()
  oWidget:CallActionScriptCallback("Start", {
    oWidget.CustomData.sTitle,
    oWidget.CustomData.sName,
    oWidget.CustomData.sJobTitle,
    oWidget.CustomData.sPhone1,
    oWidget.CustomData.sPhone2,
    oWidget.CustomData.sEmail
  })
  oWidget:SetFlashEventHandler("FanfareOff", _CleanupCardFanfare, {oWidget})
  Event.Create(Event.TimerRelative, {1}, _ContinueCardFanfare, {oWidget})
end

function _ContinueCardFanfare(oWidget)
  oWidget:CallActionScriptCallback("Continue", {})
  Event.Create(Event.TimerRelative, {
    oWidget.CustomData.nDisplayTime or 3
  }, _EndCardFanfare, {oWidget})
end

function _EndCardFanfare(oWidget)
  oWidget:CallActionScriptCallback("End", {})
end

function _CleanupCardFanfare(oWidget)
  Event.Create(Event.TimerRelative, {0.1}, _DeleteFanfareWidget, {oWidget})
end

function SetFanfareParameters(sProfileName1, sProfileName2, sCancelMsg, bAllowRetry)
  if not _gFanfareFlashWidget then
    return false
  end
  if "contract" ~= _gFanfareFlashWidget.CustomData.sType and "mission" ~= _gFanfareFlashWidget.CustomData.sType and "wager" ~= _gFanfareFlashWidget.CustomData.sType then
    return false
  end
  if "string" ~= type(sProfileName1) then
    return false
  end
  if "mission" == _gFanfareFlashWidget.CustomData.sType then
    sProfileName2 = nil
  end
  _gFanfareFlashWidget.CustomData.sProfileName1 = sProfileName1
  _gFanfareFlashWidget.CustomData.sProfileName2 = sProfileName2
  _gFanfareFlashWidget.CustomData.bAllowRetry = bAllowRetry
  if "string" == type(sCancelMsg) then
    _gFanfareFlashWidget.CustomData.sCancelMsg = sCancelMsg
  end
  return true
end

function AddFanfareLineItem(sDescription, nValue, sType, nPlayer)
  if not _gFanfareFlashWidget then
    return false
  end
  if "contract" ~= _gFanfareFlashWidget.CustomData.sType and "wager" ~= _gFanfareFlashWidget.CustomData.sType then
    return false
  end
  if "string" ~= type(sDescription) then
    return false
  end
  if "number" ~= type(nValue) then
    return false
  end
  if "number" ~= type(nPlayer) then
    nPlayer = 1
  end
  if _gFanfareFlashWidget.CustomData.sCancelMsg then
    return false
  end
  if not _gFanfareFlashWidget.CustomData.tLineList then
    _gFanfareFlashWidget.CustomData.tLineList = {}
  end
  local tLine = {
    sDescription = sDescription,
    nValue = nValue,
    sType = sType,
    nPlayer = nPlayer
  }
  table.insert(_gFanfareFlashWidget.CustomData.tLineList, tLine)
  return true
end

function CommenceFanfare(nSlowdownDuration)
  if not _gFanfareFlashWidget then
    return false
  end
  if "contract" == _gFanfareFlashWidget.CustomData.sType or "wager" == _gFanfareFlashWidget.CustomData.sType then
    if not _gFanfareFlashWidget.CustomData.sCancelMsg and not _gFanfareFlashWidget.CustomData.tLineList then
      return false
    end
  elseif "mission" == _gFanfareFlashWidget.CustomData.sType then
  else
    return false
  end
  MrxHqManager.LockAllHq()
  if Net.IsServer() then
    Net.SendEvent_Fanfare(_gFanfareFlashWidget.CustomData.sType, "", "", "", _gFanfareFlashWidget.CustomData.sCancelMsg or "", _gFanfareFlashWidget.CustomData.tLineList or {}, nSlowdownDuration or 2, 0)
  end
  nSlowdownDuration = nSlowdownDuration or 2
  _gFanfareFlashWidget.CustomData.nDilationSpeed = 1 / nSlowdownDuration
  _gFanfareFlashWidget.CustomData.nPrevTimeScale = 1
  MrxGui.AddWidget(_gFanfareFlashWidget)
  _gFanfareFlashWidget:SetEventHandler("GuiUpdate", _SlowdownUpdate)
  MrxGui.AddWidget(_gFullscreenFadeWidget)
  local bEnabled = MrxGuiManager.GetHudState(Player.GetLocalPlayer())
  _gFanfareFlashWidget.CustomData.bHudState = bEnabled
  if bEnabled then
    MrxGuiManager.ToggleHud(Player.GetLocalPlayer(), false)
  end
  local oPauseMenu = MrxGui.GetWidgetByName("Pause Layout")
  if oPauseMenu then
    oPauseMenu:SetUserSaveEnabled(false)
  end
  if not _gFanfareFlashWidget.CustomData.bSuppressedPda then
    _gFanfareFlashWidget.CustomData.bSuppressedPda = true
    Pda:SetSuppressed({vPlayer = nil, bSuppress = true})
  end
  if Player.SetScopeEnabled and Player.GetLocalPlayer() then
    Player.SetScopeEnabled(Player.GetLocalPlayer(), false)
  end
  return true
end

function NetClientCloseFanfare()
  if not _gFanfareFlashWidget then
    return false
  end
  _gFanfareFlashWidget:CallActionScriptCallback("soundMoneyGainStop", {})
  _gFanfareFlashWidget:CallActionScriptCallback("soundMoneyLoseStop", {})
  _gFanfareFlashWidget:CallActionScriptCallback("CloseFanfare", {})
  _EndFanfare(_gFanfareFlashWidget, "false")
end

function _SlowdownUpdate(oWidget, nDeltaTime)
  local nTimeSpeed = _gFanfareFlashWidget.CustomData.nDilationSpeed
  local nTimeScale = oWidget.CustomData.nPrevTimeScale
  nTimeScale = nTimeScale - nTimeSpeed * nDeltaTime
  if nTimeScale <= 0 then
    nTimeScale = 0
    _BeginFanfareFlash(oWidget)
    oWidget:SetEventHandler("GuiUpdate", nil)
  end
  oWidget.CustomData.nPrevTimeScale = nTimeScale
  local nAlphaMax = 192
  _gFullscreenFadeWidget.CustomData.nAlpha = _gFullscreenFadeWidget.CustomData.nAlpha + nDeltaTime * nTimeSpeed * nAlphaMax
  if nAlphaMax < _gFullscreenFadeWidget.CustomData.nAlpha then
    _gFullscreenFadeWidget.CustomData.nAlpha = nAlphaMax
  end
  _gFullscreenFadeWidget:SetColor(0, 0, 0, _gFullscreenFadeWidget.CustomData.nAlpha)
end

function _LoadCompleteCallback(oWidget)
  oWidget.CustomData.bLoadingComplete = true
  if oWidget.CustomData.bReadyToStart then
    _BeginFanfareFlash(oWidget)
  end
end

function _BeginFanfareFlash(oWidget)
  if oWidget.CustomData.bLoadingComplete then
    oPauseScreen = MrxGuiBase.GetWidgetByName("Pause Layout")
    if oPauseScreen then
      oPauseScreen:Close()
      Sys.RequestGameState("ingame")
    end
    if Player.GetLocalCharacter() then
      Object.SetInvincible(Player.GetLocalCharacter(), true, "Fanfare")
    end
    _InitializeFanfareFlash(oWidget)
    Event.Create(Event.TimerRelative, {0.01, true}, _InitialDelay, {oWidget})
    MrxGuiBase.GetControlFocus(oWidget, false)
  else
    oWidget.CustomData.bReadyToStart = true
  end
end

function _InitializeFanfareFlash(oWidget)
  local tData = oWidget.CustomData
  LTILibName.ChangeShellState(true)
  oWidget:SetVisible(true)
  oWidget:Pause()
  local sCancelMsg = tData.sCancelMsg or nil
  local bCompleted = tData.sCancelMsg == nil
  local nPlayers = 1
  if tData.sProfileName2 then
    nPlayers = 2
  end
  if bCompleted then
    oWidget:CallActionScriptCallback("fanfareInitialize", {
      bCompleted,
      tData.bAllowRetry,
      nPlayers,
      " ",
      not Net.IsClient()
    })
  else
    oWidget:CallActionScriptCallback("fanfareInitialize", {
      false,
      tData.bAllowRetry and not Net.IsClient(),
      nPlayers,
      sCancelMsg,
      not Net.IsClient()
    })
  end
  if not tData.tLineList then
    tData.tLineList = {}
  end
  if bCompleted then
    oWidget:CallActionScriptCallback("AddProfile1Name", {
      tData.sProfileName1
    })
    local nPositive = 1
    local nValue
    for nIndex, tLine in pairs(tData.tLineList) do
      if 1 == tLine.nPlayer then
        if tLine.nValue < 0 then
          nValue = tLine.nValue * -1
          nPositive = 0
        else
          nValue = tLine.nValue
          nPositive = 1
        end
        oWidget:CallActionScriptCallback("AddProfile1Data", {
          tLine.sDescription,
          nValue,
          nPositive,
          tLine.sType
        })
      end
    end
    if tData.sProfileName2 then
      oWidget:CallActionScriptCallback("AddProfile2Name", {
        tData.sProfileName2
      })
      local nPositive = 1
      local nValue
      for nIndex, tLine in pairs(tData.tLineList) do
        if 2 == tLine.nPlayer then
          if tLine.nValue < 0 then
            nValue = tLine.nValue * -1
            nPositive = 0
          else
            nValue = tLine.nValue
            nPositive = 1
          end
          oWidget:CallActionScriptCallback("AddProfile2Data", {
            tLine.sDescription,
            nValue,
            nPositive,
            tLine.sType
          })
        end
      end
    end
  end
  oWidget:SetFlashEventHandler("closeFanfare", _EndFanfare, {oWidget})
  oWidget:SetFlashEventHandler("Retry", _EndFanfare, {oWidget})
  oWidget:SetFlashEventHandler("FanfareCountUpComplete", _ContinueFanfare, {oWidget})
end

function _ContinueFanfare(oWidget)
  oWidget:CallActionScriptCallback("fanfareButtonsAppear", {})
end

function _InitialDelay(oWidget)
  oWidget:SetVisible(true)
  oWidget:Play()
end

function _SkipFanfare(oWidget)
  if Sys.IsConfirmOnCircle and Sys.IsConfirmOnCircle() then
    _evSkip = Event.Create(Event.Button, {
      Player.GetLocalPlayer(),
      "cancel",
      "press",
      true
    }, _EndFanfare, {oWidget})
  else
    _evSkip = Event.Create(Event.Button, {
      Player.GetLocalPlayer(),
      "selection",
      "press",
      true
    }, _EndFanfare, {oWidget})
  end
end

function _RetryEvents(oWidget)
  _evLSLeft = Event.CreatePersistent(Event.Button, {
    Player.GetLocalPlayer(),
    "lsleft",
    "press",
    true
  }, _GuiInternal.SendFlashInput, {
    oWidget.BasicData.uId,
    9,
    "p"
  })
  _evLSRight = Event.CreatePersistent(Event.Button, {
    Player.GetLocalPlayer(),
    "lsright",
    "press",
    true
  }, _GuiInternal.SendFlashInput, {
    oWidget.BasicData.uId,
    10,
    "p"
  })
  if Sys.IsConfirmOnCircle and Sys.IsConfirmOnCircle() then
    _evSelect = Event.Create(Event.Button, {
      Player.GetLocalPlayer(),
      "cancel",
      "press",
      true
    }, _GuiInternal.SendFlashInput, {
      oWidget.BasicData.uId,
      6,
      "p"
    })
  else
    _evSelect = Event.CreatePersistent(Event.Button, {
      Player.GetLocalPlayer(),
      "selection",
      "press",
      true
    }, _GuiInternal.SendFlashInput, {
      oWidget.BasicData.uId,
      6,
      "p"
    })
  end
end

function OnPlayerJoined()
  if _gFanfareFlashWidget then
    _GuiInternal.SendFlashInput(_gFanfareFlashWidget.BasicData.uId, MrxGuiBase.Joystick.BUTTON_PAD2_D, "p")
  end
end

function _DeleteFanfareEvents()
  Event.Delete(_evSurvivalMode)
  Event.Delete(_evAutoCancel)
  Event.Delete(_evSkip)
  Event.Delete(_evLSLeft)
  Event.Delete(_evLSRight)
  Event.Delete(_evSelect)
end

function _EndFanfare(oWidget, sEnd)
  if not _gFanfareFlashWidget then
    return
  end
  if Net.IsServer() and Net.SendEvent_CloseFanfare then
    Net.SendEvent_CloseFanfare()
  end
  LTILibName.ChangeShellState(false)
  if _gFanfareFlashWidget.CustomData.bHudState then
    MrxGuiManager.ToggleHud(Player.GetLocalPlayer(), true)
  end
  _gFanfareFlashWidget:CallActionScriptCallback("requestClose", {})
  _gFanfareFlashWidget = nil
  MrxGuiBase.ReleaseControlFocus(oWidget)
  if "true" == sEnd then
    oWidget.CustomData.bRetry = false
  elseif "false" == sEnd then
    oWidget.CustomData.bRetry = true
  end
  Event.Create(Event.TimerRelative, {0.01}, _DeleteFanfareWidget, {oWidget})
  MrxGui.RemoveWidget(_gFullscreenFadeWidget)
  _gFullscreenFadeWidget:delete()
  if Player.GetLocalCharacter() then
    Object.SetInvincible(Player.GetLocalCharacter(), false, "Fanfare")
  end
  MrxHqManager.UnlockAllHq()
end

function _DeleteFanfareWidget(oWidget)
  local fCallback = oWidget.CustomData.fCallback
  local tData = oWidget.CustomData.tCallbackData
  local bRetry = oWidget.CustomData.bRetry
  if _gFanfareFlashWidget == oWidget then
    _gFanfareFlashWidget = nil
  end
  oWidget:SetSwfFile(nil)
  MrxGui.RemoveWidget(oWidget)
  oWidget:delete()
  local oPauseMenu = MrxGui.GetWidgetByName("Pause Layout")
  if oPauseMenu then
    oPauseMenu:SetUserSaveEnabled(true)
  end
  if oWidget.CustomData.bSuppressedPda then
    Pda:SetSuppressed({vPlayer = nil, bSuppress = false})
    oWidget.CustomData.bSupressedPda = nil
  end
  if Player.SetScopeEnabled and Player.GetLocalPlayer() then
    Player.SetScopeEnabled(Player.GetLocalPlayer(), true)
  end
  if "function" == type(fCallback) then
    if "table" ~= type(tData) then
      tData = {}
    end
    if "contract" == oWidget.CustomData.sType or "mission" == oWidget.CustomData.sType or "wager" == oWidget.CustomData.sType then
      table.insert(tData, bRetry)
    end
    fCallback(unpack(tData))
  end
end

function _HandleFanfareInput(oWidget, tEvent)
  oWidget.CustomData.HandleFlashInput(oWidget, tEvent)
end

function ShowTextFanfare(uPlayerGuid, sLine1, sLine2, nEnterTime, nDisplayTime, nFadeTime, fCallback, tCallbackData)
  if "string" ~= type(sLine1) then
    return false
  end
  if "string" ~= type(sLine2) then
    return false
  end
  local oTextFanfare = MrxGui.Widget:new()
  oTextFanfare:SetAnchoring("center", "center")
  oTextFanfare:SetLocation(0, 0, 640, 480)
  local oText1 = MrxGui.TextWidget:new()
  oText1:SetFont("fanfare_36")
  oText1:SetScale(1)
  oText1:SetAnchoring("center", "center")
  oText1:SetText(sLine1)
  oText1:SetOwner(uPlayerGuid)
  local oText2 = MrxGui.TextWidget:new()
  oText2:SetFont("fanfare_36")
  oText2:SetScale(1)
  oText2:SetAnchoring("center", "center")
  oText2:SetText(sLine2)
  oText2:SetOwner(uPlayerGuid)
  oTextFanfare:AddChild(oText1)
  oTextFanfare:AddChild(oText2)
  local nLine1W = oText1:GetWidth()
  local nLine1H = oText1:GetHeight()
  local nLine2W = oText2:GetWidth()
  local nLine2H = oText2:GetHeight()
  local nTargetX = 320
  local nTargetY = 240
  local nLine1TargetX = nTargetX - nLine1W * 0.5
  local nLine1TargetY = nTargetY - nLine1H * 1
  local nLine2TargetX = nTargetX - nLine2W * 0.5
  local nLine2TargetY = nTargetY + nLine2H * 0
  local nLine1StartX = 0 - nLine1W
  local nLine2StartX = 640
  oText1:SetLocation(nLine1StartX, nLine1TargetY)
  oText2:SetLocation(nLine2StartX, nLine2TargetY)
  local nText1Point = oText1:AddAnimationPoint({x = nLine1TargetX})
  local nText2Point = oText2:AddAnimationPoint({x = nLine2TargetX})
  local nTime = nEnterTime or 0.25
  local nDelay = nDisplayTime or 3
  local nFade = nFadeTime or 1
  MrxGui.AddWidget(oText1)
  MrxGui.AddWidget(oText2)
  oText1:AnimateToPoint(nText1Point, nTime, true, _TextDelay, {
    oTextFanfare,
    nDelay,
    nFade
  })
  oText2:AnimateToPoint(nText2Point, nTime, true, _TextDelay, {
    oTextFanfare,
    nDelay,
    nFade
  })
  oTextFanfare.CustomData.uTimerEvent = Event.Create(Event.TimerRelative, {
    nTime + nDelay + nFade + 0.1
  }, _TextFanfareDone, {
    oTextFanfare,
    fCallback,
    tCallbackData
  })
  if Net.IsServer() then
    Net.SendEvent_TextFanfare(sLine1, sLine2, nTime, nDelay, nFade)
  end
end

function _TextFanfareDone(oTextFanfare, fCallback, tCallbackData)
  if "function" == type(fCallback) then
    fCallback(tCallbackData and unpack(tCallbackData))
  end
  MrxGui.RemoveWidget(oTextFanfare)
  oTextFanfare:delete()
end

function _TextDelay(oWidget, oTextFanfare, nDelayTime, nFadeTime)
  Event.Create(Event.TimerRelative, {nDelayTime}, _TextFadeout, {
    oWidget,
    oTextFanfare,
    nFadeTime
  })
end

function _TextFadeout(oWidget, oTextFanfare, nFadeTime)
  local nPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 0})
  oWidget:AnimateToPoint(nPoint, nFadeTime, true, _TextDelete, {oTextFanfare})
end

function _TextDelete(oWidget, oTextFanfare)
  oTextFanfare:RemoveChild(oWidget)
  MrxGui.RemoveWidget(oWidget)
  oWidget:delete()
end

_tEventTitles = {
  contact = "[Fanfare.Common.NewContact]",
  support = "[Fanfare.Common.NewShopItem]",
  stockpile = "[Fanfare.Common.NewStockpileItem]",
  landingzone = "[Fanfare.Common.NewLandingZone]",
  hvtcapture = "[Fanfare.Common.HvtCaptured]",
  hvtkill = "[Fanfare.Common.HvtKilled]",
  bounty = "[Fanfare.Common.NewBounties]",
  outfit = "[Fanfare.Common.NewOutfit]",
  highscore = "[Fanfare.Common.NewHighScore]"
}
_tEventTextures = {
  contact = "unlockables_newcontact",
  support = "unlockables_newshopitem",
  stockpile = "unlockables_newstockpileitem",
  landingzone = "unlockables_landingzone",
  hvtcapture = "unlockables_hvtcaptured",
  hvtkill = "unlockables_hvtkilled",
  bounty = "unlockables_newbounties",
  outfit = "unlockables_newoutfit",
  highscore = "unlockables_leaderboardupdated"
}
_tEventSounds = {
  contact = "ui_signal_ding",
  support = "ui_signal_ding",
  stockpile = "ui_signal_ding",
  landingzone = "ui_signal_ding",
  hvtcapture = "ui_signal_generic",
  hvtkill = "ui_signal_generic",
  bounty = "ui_signal_ding",
  outfit = "ui_signal_ding",
  highscore = "ui_signal_ding"
}
_tEventTextureWidths = {
  contact = 398,
  support = 456,
  stockpile = 512,
  landingzone = 512,
  hvtcapture = 432,
  hvtkill = 365,
  bounty = 428,
  outfit = 370,
  highscore = 512
}
_knTextQueueFadeTime = 0.5

function ShowEventFanfare(sType, vText, fCallback, tCallbackData)
  if not sType or not _tEventTextures[sType] then
    return
  end
  if "string" ~= type(vText) and "table" ~= type(vText) then
    return
  end
  local sText, tTextQueue
  local nDisplayTime = 4
  local nTotalTime = nDisplayTime
  if "string" == type(vText) then
    sText = vText
  elseif "table" == type(vText) then
    local nItemCount = #vText
    nDisplayTime = math.max(1.5, 4 / nItemCount)
    nTotalTime = (nDisplayTime + _knTextQueueFadeTime) * nItemCount + _knTextQueueFadeTime
    sText = vText[1]
    tTextQueue = vText
    table.remove(vText, 1)
  end
  local nScale = 0.75
  local nWidth = 512 * nScale
  local oContainer = MrxGui.Widget:new()
  oContainer:SetLocation(0, 0, 512, 128)
  oContainer.CustomData.fCallback = fCallback
  oContainer.CustomData.tCallbackData = tCallbackData
  local oIcon = MrxGui.ImageWidget:new()
  oIcon:SetLocation(0, 0, 512 * nScale, 128 * nScale)
  oIcon:SetTexture(_tEventTextures[sType])
  _GuiInternal.SetImageTextureTransience(oIcon.BasicData.uId, true)
  local oInfo = MrxGui.TextWidget:new()
  oInfo:SetLocation(72 * nScale, 102 * nScale, 512 * nScale, 128 * nScale)
  oInfo:SetFont("english_18")
  oInfo:SetText(sText)
  local nMaxWidth = oInfo:GetWidth()
  if tTextQueue then
    for n, s in ipairs(tTextQueue) do
      oInfo:SetText(s)
      nMaxWidth = math.max(nMaxWidth, oInfo:GetWidth())
    end
    oInfo:SetText(sText)
  end
  nWidth = math.max(nMaxWidth + 72 * nScale, nWidth)
  oInfo.CustomData.tTextQueue = tTextQueue
  oInfo.CustomData.nDisplayTime = nDisplayTime
  oInfo.CustomData.nFadePoint = oInfo:AddAnimationPoint({TranslucencyLevel = 0})
  oInfo.CustomData.nBasePoint = oInfo:AddAnimationPoint({TranslucencyLevel = 255})
  oContainer:SetLocation(0, 0, nWidth, 128 * nScale)
  oContainer:AddChild(oIcon)
  oContainer:AddChild(oInfo)
  oContainer.CustomData.oIcon = oIcon
  oContainer.CustomData.oInfo = oInfo
  local uPlayerGuid = Player.GetLocalPlayer()
  oContainer:SetOwner(uPlayerGuid)
  oIcon:SetOwner(uPlayerGuid)
  oInfo:SetOwner(uPlayerGuid)
  local nPoint = oContainer:AddAnimationPoint({
    x = 320 - nWidth * 0.5,
    y = 156
  })
  oContainer:SetAnchoring("center", "center")
  oContainer:SetLocation(320 - nWidth * 0.5, 640)
  MrxGui.AddWidgetWithChildren(oContainer)
  oContainer.CustomData.sType = sType
  oContainer.CustomData.nTime = nTotalTime
  oContainer:AnimateToPoint(nPoint, 0.5, true, _EventFanfareFinishAppear)
end

function _EventFanfareFinishAppear(oFanfare)
  if _tEventSounds[oFanfare.CustomData.sType] then
    Sound.CueSound(0, _tEventSounds[oFanfare.CustomData.sType])
  end
  local nPoint = oFanfare:AddAnimationPoint({TranslucencyLevel = 255})
  oFanfare:AnimateToPoint(nPoint, oFanfare.CustomData.nTime, true, _EventFanfareFinishDisplay)
  local oInfo = oFanfare.CustomData.oInfo
  if oInfo.CustomData.tTextQueue then
    oInfo:AnimateToPoint(oInfo.CustomData.nBasePoint, oInfo.CustomData.nDisplayTime, true)
    oInfo:AnimateToPoint(oInfo.CustomData.nFadePoint, _knTextQueueFadeTime * 0.5, false, _EventFanfareProcessTextQueue)
  end
end

function _EventFanfareProcessTextQueue(oInfo)
  if oInfo.CustomData.tTextQueue then
    local sText = oInfo.CustomData.tTextQueue[1]
    if not sText then
      oInfo.CustomData.tTextQueue = nil
      return
    end
    table.remove(oInfo.CustomData.tTextQueue, 1)
    if #oInfo.CustomData.tTextQueue < 1 then
      oInfo.CustomData.tTextQueue = nil
    end
    oInfo:SetText(sText)
    oInfo:AnimateToPoint(oInfo.CustomData.nBasePoint, _knTextQueueFadeTime * 0.5, true, _EventFanfareContinueTextFade)
  end
end

function _EventFanfareContinueTextFade(oInfo)
  if oInfo.CustomData.tTextQueue then
    oInfo:AnimateToPoint(oInfo.CustomData.nBasePoint, oInfo.CustomData.nDisplayTime, true)
    oInfo:AnimateToPoint(oInfo.CustomData.nFadePoint, _knTextQueueFadeTime * 0.5, false, _EventFanfareProcessTextQueue)
  end
end

function _EventFanfareFinishDisplay(oFanfare)
  local nPoint = oFanfare:AddAnimationPoint({TranslucencyLevel = 0})
  oFanfare:AnimateToPoint(nPoint, 1, true, _EventFanfareComplete)
end

function _EventFanfareComplete(oFanfare)
  local fCallback = oFanfare.CustomData.fCallback
  local tData = oFanfare.CustomData.tCallbackData
  MrxGui.RemoveWidgetWithChildren(oFanfare)
  oFanfare.CustomData.oIcon:SetTexture(nil)
  oFanfare.CustomData.oIcon:delete()
  oFanfare.CustomData.oInfo:delete()
  oFanfare:delete()
  if "function" == type(fCallback) then
    if "table" ~= type(tData) then
      tData = {}
    end
    fCallback(unpack(tData))
  end
end

function GetEventFanfareTitle(sType)
  return _tEventTitles[sType]
end

function HandleInitialization(oWidget, tUnused)
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  nGlobalWidth = nX2 - nX1
  nGlobalHeight = nY2 - nY1
  nX = nX1 + nGlobalWidth / 2
  nY = nY1 + nGlobalHeight / 2
  oWidget.ParentWidget:SetTranslucency(0)
  Event.Create(Event.TimerRelative, {0.5}, MrxGui.RemoveWidget, {
    oWidget.ParentWidget
  })
end

function ShowCompletedMessage(uPlayerGuid, fZoomCompleteCallback, fFadeCompleteCallback)
  ShowMessage(uPlayerGuid, "global_gui_completed", fZoomCompleteCallback, fFadeCompleteCallback)
end

function ShowFailedMessage(uPlayerGuid, fZoomCompleteCallback, fFadeCompleteCallback)
  ShowMessage(uPlayerGuid, "global_gui_failed", fZoomCompleteCallback, fFadeCompleteCallback)
end

function ShowMessage(uPlayerGuid, sTextureName, fZoomCompleteCallback, fFadeCompleteCallback, nXLocation, nYLocation, sHorizontalAnchor, sVerticalAnchor, nMessageWidth, nMessageHeight, nMessageDisplayTime, vSoundEffect)
  local nTextureHash
  if "userdata" ~= type(uPlayerGuid) and nil ~= uPlayerGuid then
    return
  end
  if "string" ~= type(sTextureName) and "userdata" ~= type(sTextureName) then
    return
  end
  if Net.IsServer() and not Player.IsLocal(uPlayerGuid) then
    Net.SendEvent_ShowMessage(uPlayerGuid, sTextureName, nXLocation or nX, nYLocation or nY, sHorizontalAnchor or "center", sVerticalAnchor or "center", nMessageWidth or nGlobalWidth, nMessageHeight or nGlobalTime, nMessageDisplayTime or nLifeTime)
    return
  end
  local oMessage = MrxGui.ImageWidget:new()
  local nWidth = nMessageWidth or nGlobalWidth
  local nHeight = nMessageHeight or nGlobalHeight
  local nTime = nMessageDisplayTime or nLifeTime
  local nPoint = oMessage:AddAnimationPoint({
    x = (nXLocation or nX) - nWidth / 2,
    y = (nYLocation or nY) - nHeight / 2,
    x1 = (nXLocation or nX) + nWidth / 2,
    y1 = (nYLocation or nY) + nHeight / 2,
    TranslucencyLevel = 255,
    nAnimationTime = nAnimationTimeLength
  })
  oMessage:SetTranslucency(96)
  oMessage:SetTexture(sTextureName)
  oMessage:SetLocation(nX - nWidth * nMagnification, nY - nHeight * nMagnification, nX + nWidth * nMagnification, nY + nHeight * nMagnification)
  oMessage:SetAnchoring(sHorizontalAnchor or "center", sVerticalAnchor or "center")
  oMessage:SetOwner(uPlayerGuid)
  oMessage:SetName("Announcement")
  oMessage.CustomData.fZoomCompleteCallback = fZoomCompleteCallback
  oMessage.CustomData.fFadeCompleteCallback = fFadeCompleteCallback
  oMessage.CustomData.nDisplayTime = nTime
  _GuiInternal.SetImageTextureTransience(oMessage.BasicData.uId, true)
  local tSoundEffect
  if "table" ~= type(vSoundEffect) then
    tSoundEffect = {vSoundEffect}
  else
    tSoundEffect = vSoundEffect
  end
  for nIndex, sSound in pairs(tSoundEffect) do
    if "string" == type(sSound) then
      Sound.CueSound(0, sSound)
    end
  end
  MrxGui.AddWidget(oMessage)
  oMessage:AnimateToPoint(nPoint, nil, true, AnimationFinishCallback)
  return oMessage
end

function AnimationFinishCallback(oWidget)
  if oWidget.CustomData.nDisplayTime >= 0 then
    oWidget:SetEventHandler("GuiUpdate", HandleUpdateEvent)
    oWidget.CustomData.nTimeRemaining = oWidget.CustomData.nDisplayTime
  end
  if "function" == type(oWidget.CustomData.fZoomCompleteCallback) then
    oWidget.CustomData.fZoomCompleteCallback(oWidget)
  end
end

function HandleUpdateEvent(oWidget, nTimeSinceLastUpdate)
  oWidget.CustomData.nTimeRemaining = oWidget.CustomData.nTimeRemaining - nTimeSinceLastUpdate
  if oWidget.CustomData.nTimeRemaining < 0 then
    oWidget:SetEventHandler("GuiUpdate", nil)
    local nPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 0, nAnimationTime = nFadeTime})
    oWidget:AnimateToPoint(nPoint, nil, true, RemovalCallback)
  end
end

function RemovalCallback(oWidget)
  if "function" == type(oWidget.CustomData.fFadeCompleteCallback) then
    oWidget.CustomData.fFadeCompleteCallback(oWidget)
  end
  oWidget:SetTexture(nil)
  MrxGui.RemoveWidget(oWidget)
  oWidget:delete()
end

function Init()
  Gui.LoadFont("fanfare_36")
end

_nClassyTextWidth = 566.6667
_nClassyTextHeight = 33.333336

function HandleClassyTextInit(oWidget)
  oWidget:SetFullscreen(true)
  oWidget.ShowText = DisplayClassyText
end

function DisplayClassyText(oWidget, sText, nX, nY, nDuration, nScale, sHorizAnchor, sVertAnchor, sJustification, bExpand)
  if "string" ~= type(sText) then
    return
  end
  nX = (640 - _nClassyTextWidth) * 0.5
  nY = nY or 240
  nDuration = nDuration or 3
  sHorizAnchor = sJustification or "left"
  sVertAnchor = sVertAnchor or "center"
  sJustification = sJustification or "left"
  bExpand = bExpand or false
  nDuration = nDuration * 30
  local oFlash = MrxGuiBase.FlashWidget:new()
  oWidget:AddChild(oFlash)
  oFlash:SetOwner(oWidget:GetOwner())
  oWidget:AddChild(oFlash)
  oFlash.ParentWidget = oWidget
  
  function Clamp(n, nMin, nMax)
    return (Math.max(Math.min(n, nMax), nMin))
  end
  
  nScale = 1
  local nWidth = _nClassyTextWidth * nScale
  local nHeight = _nClassyTextHeight * nScale
  local nRealX = nX
  nRealX = Clamp(nRealX, 0, 600)
  nWidth = Clamp(nWidth, 1, 640 - nRealX)
  local nRealY = nY
  if "center" == sVertAnchor then
    nRealY = nY - nHeight * 0.5
  elseif "bottom" == sVertAnchor then
    nRealY = nY - nHeight
  end
  if nRealY < 0 or 480 < nRealY + nHeight then
    Debug.Printf("Text bounding box outside of the screen, text may be squished.")
  end
  nRealY = Clamp(nRealY, 0, 450)
  nHeight = Clamp(nHeight, 1, 480 - nRealY)
  oFlash:SetLocation(nRealX, nRealY, nRealX + nWidth, nRealY + nHeight)
  oFlash:SetAnchoring(sHorizAnchor, sVertAnchor)
  oFlash:SetSwfFile("text_effect", _ClassyTextLoadCompleteCallback, {
    oFlash,
    sText,
    nDuration,
    sJustification,
    bExpand
  })
  MrxGuiBase.AddWidget(oFlash)
end

function _ClassyTextLoadCompleteCallback(oText, sText, nDuration, sJustification, bExpand)
  oText:SetFlashEventHandler("close", _HandleClassyTextEnd)
  local nExpand = 1
  if not bExpand then
    nExpand = 2
  end
  local nJustification = 1
  if "center" == sJustification then
    nJustification = 2
  elseif "right" == sJustification then
    nJustification = 3
  end
  oText:CallActionScriptCallback("textInput", {
    sText,
    nExpand,
    nJustification,
    nDuration
  })
end

function _HandleClassyTextEnd(oText)
  oText:SetVisible(false)
  Event.Create(Event.TimerRelative, {0.05, true}, _DeleteClassyText, {oText})
end

function _DeleteClassyText(oText)
  oText:SetSwfFile(nil)
  oText.ParentWidget:RemoveChild(oText)
  oText.ParentWidget = nil
  MrxGuiBase.RemoveWidget(oText)
  oText:delete()
end
