import("MrxGuiBase")
import("MrxGui")
import("MrxGuiManager")
import("MrxSound")
local bShowMoviePending

function Show(oWidget, sTexture, sText, nFadeInTime, nFadeOutTime, fCallback, tCallbackData)
  if "string" ~= type(sText) then
    sText = " "
  end
  if "string" ~= type(sTexture) then
    sTexture = " "
  end
  if "number" ~= type(nFadeInTime) then
    nFadeInTime = 0.2
  end
  if "number" ~= type(nFadeOutTime) then
    nFadeOutTime = 0.2
  end
  oWidget.CustomData.fCallback = fCallback
  oWidget.CustomData.tCallbackData = tCallbackData
  oWidget.CustomData.nFadeOutTime = nFadeOutTime
  local tChildren = oWidget:GetChildren()
  local oPicture = tChildren[2]
  oPicture:SetTexture(sTexture)
  oWidget.CustomData.oShowWidget = nil
  tChildren[3]:SetText(sText)
  tChildren[3]:Wrap()
  for nIndex, oChild in pairs(tChildren) do
    MrxGuiBase.AddWidget(oChild)
  end
  local nBottom = 400
  local nX = tChildren[3]:GetLocation()
  tChildren[3]:SetLocation(nX, nBottom - tChildren[3]:GetHeight())
  oWidget:SetVisible(true)
  local oBackWidget = oWidget:GetChildren()[1]
  oBackWidget:AnimateToPoint(oBackWidget.CustomData.nFadeInPoint, nFadeInTime, true, _FadeInElements, {
    oWidget,
    nFadeInTime,
    nil,
    1
  })
  oWidget:SetEventHandler("ControllerInput", _HandleInputEvent)
  MrxGuiBase.PushWidgetToFront(oWidget)
end

function IsMovieRunning(oWidget)
  return oWidget:GetVisible() or oWidget.CustomData.nFadeOutTime
end

function IsMovieHiding(oWidget)
  return oWidget.bSlowHiding == true
end

function ShowMovie(oWidget, sFile, nFadeInTime, nFadeOutTime, fCallback, tCallbackData, bSubtitles, tSubtitles)
  Debug.Printf("MrxGuiCinematic.ShowMovie " .. sFile)
  if oWidget:GetVisible() or oWidget.CustomData.nFadeOutTime then
    bShowMoviePending = true
    Debug.Printf("-- movie is already playing")
    oWidget.retryEvent = Event.Create(Event.TimerRelative, {0.05}, ShowMovie, {
      oWidget,
      sFile,
      nFadeInTime,
      nFadeOutTime,
      fCallback,
      tCallbackData,
      bSubtitles,
      tSubtitles
    })
    return
  end
  if bSubtitles and not tSubtitles then
    local sSubtitleFilename = "Subtitles_" .. sFile
    if sFile == "01_VIK_01" then
      sSubtitleFilename = "TECHNOV"
    end
    Debug.Printf("Dynamic importing: " .. sSubtitleFilename)
    oWidget.bSubtitlesLoading = true
    dynamic_import(sSubtitleFilename, SubtitleImportCallback, {
      {
        oWidget,
        sFile,
        nFadeInTime,
        nFadeOutTime,
        fCallback,
        tCallbackData,
        bSubtitles
      }
    })
    return
  end
  if Net.IsClient() and bSubtitles and not oWidget.bSubtitlesLoading then
    return
  end
  if Net.IsClient() and not bShowMoviePending then
    Debug.Printf("BLACKSCREEN - FadeToColor called from MrxGuiCinematic.ShowMovie")
    MrxGui.FadeToColor(0, nil, 0, 0, 0)
  end
  bShowMoviePending = nil
  local tSubtitleData = tSubtitles
  Debug.Printf("-- starting movie")
  local sText = " "
  if "string" ~= type(sTexture) then
    sTexture = " "
  end
  if "number" ~= type(nFadeInTime) then
    nFadeInTime = 0.2
  end
  if "number" ~= type(nFadeOutTime) then
    nFadeOutTime = 0.2
  end
  oWidget.CustomData.fCallback = fCallback
  oWidget.CustomData.tCallbackData = tCallbackData
  oWidget.CustomData.nFadeOutTime = nFadeOutTime
  local oMovie = oWidget.CustomData.oMovieWidget
  Debug.Printf("Setting movie to " .. sFile)
  oMovie:SetMovie(sFile)
  oWidget.CustomData.oShowWidget = oMovie
  local oPlaceholderText = oWidget.CustomData.oPlaceholderText
  oPlaceholderText:SetText(sText)
  oPlaceholderText:Wrap()
  for nIndex, oChild in pairs(oWidget.CustomData.tAddOrder) do
    MrxGuiBase.AddWidget(oChild)
  end
  local nBottom = 400
  local nX = oPlaceholderText:GetLocation()
  oPlaceholderText:SetLocation(nX, nBottom - oPlaceholderText:GetHeight())
  oWidget:SetVisible(true)
  oWidget.CustomData.oPicture:SetVisible(false)
  local oBackWidget = oWidget.CustomData.oBackWidget
  oBackWidget:AnimateToPoint(oBackWidget.CustomData.nFadeInPoint, nFadeInTime, true, _FadeInElements, {
    oWidget,
    nFadeInTime,
    tSubtitleData,
    1
  })
  oWidget:SetEventHandler("ControllerInput", _HandleInputEvent)
  if not Net.IsClient() then
    oMovie:SetEndCallback(HideSlow, {oWidget})
  end
  if Net.IsServer() then
    Net.SendEvent_ShowMovie(sFile, nFadeInTime, nFadeOutTime, bSubtitles)
  end
end

function SubtitleImportCallback(tArgs, oModule)
  local tSubtitleData
  if oModule then
    tSubtitleData = oModule.SubtitleData
    if "table" ~= type(tSubtitleData) then
      tSubtitleData = nil
    end
    dynamic_remove(oModule)
  end
  ShowMovie(tArgs[1], tArgs[2], tArgs[3], tArgs[4], tArgs[5], tArgs[6], tArgs[7], tSubtitleData)
end

function PlayMovie(oWidget)
  if oWidget.CustomData.oShowWidget then
    oWidget.CustomData.oShowWidget:Play()
  end
end

function PauseMovie(oWidget)
  if oWidget.CustomData.oShowWidget then
    oWidget.CustomData.oShowWidget:Pause()
  end
end

function _Hide(oWidget)
  if Net.IsClient() then
    oWidget.bSubtitlesLoading = nil
  end
  oWidget.bSlowHiding = nil
  if oWidget.retryEvent then
    Event.Delete(oWidget.retryEvent)
    oWidget.retryEvent = nil
  end
  Debug.Printf("MrxGuiCinematic._Hide")
  if Net.IsServer() then
    Net.SendEvent_HideMovie()
  end
  if Net.IsClient() then
    Debug.Printf("BLACKSCREEN - FadeFromColor called from MrxGuiCinematic._Hide")
    MrxGui.FadeFromColor()
  end
  oWidget:SetVisible(false)
  oWidget:SetTranslucency(0)
  MrxGuiBase.ReleaseControlFocus(oWidget, nil, bWasGlobal)
  oWidget:SetEventHandler("ControllerInput", nil)
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    MrxGuiBase.RemoveWidget(oChild)
  end
  oWidget.CustomData.nFadeOutTime = nil
  local oPicture = oWidget:GetChildren()[2]
  oPicture:SetTexture(nil)
  if oWidget.CustomData.oShowWidget then
    MrxGui.SetFadeEnabled(true)
    for uPlayer, bNeedsRestore in pairs(oWidget.CustomData.tHudStates) do
      if bNeedsRestore then
        MrxGuiManager.ToggleHud(uPlayer, true)
        oWidget.CustomData.tHudStates[uPlayer] = nil
      end
    end
    Debug.Printf("-- calling Stop")
    oWidget.CustomData.oShowWidget:Stop()
    oWidget.CustomData.oShowWidget = nil
  end
  if oWidget.CustomData.oSubtitle.StopSubtitles then
    oWidget.CustomData.oSubtitle:StopSubtitles()
  end
  MrxGui.SetGlobalFadeVisible(true)
  if oWidget.CustomData.fCallback then
    local fCallback = oWidget.CustomData.fCallback
    local tData
    if "table" == type(oWidget.CustomData.tCallbackData) then
      tData = oWidget.CustomData.tCallbackData
    else
      tData = {}
    end
    oWidget.CustomData.fCallback = nil
    oWidget.CustomData.tCallbackData = nil
    fCallback(unpack(tData))
  end
end

function HideSlow(oWidget)
  Debug.Printf("MrxGuiCinematic.HideSlow")
  Sys.RequestGameState("ingame")
  MrxSound.ExitCinematicState()
  if Net.IsClient() then
    oWidget.bSubtitlesLoading = nil
  end
  MrxGuiBase.ReleaseControlFocus(oWidget, nil, true)
  oWidget:SetEventHandler("ControllerInput", nil)
  local nFadeOutTime = oWidget.CustomData.nFadeOutTime or 0.2
  if nFadeOutTime <= 0 then
    Debug.Printf("-- calling _Hide")
    oWidget:_Hide()
    return
  end
  Debug.Printf("-- fading out")
  if oWidget.bSlowHiding and Net.IsClient() then
    Debug.Printf("BLACKSCREEN - FadeFromColor called from MrxGuiCinematic._Hide")
    MrxGui.FadeFromColor()
  end
  oWidget.bSlowHiding = true
  oWidget:AnimateToPoint(oWidget.CustomData.nFadeOutPoint, nFadeOutTime + 0.1, true, _Hide, {})
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    oChild:AnimateToPoint(oChild.CustomData.nFadeOutPoint, nFadeOutTime, true)
  end
  if oWidget.CustomData.oShowWidget then
    Debug.Printf("-- calling Stop")
    oWidget.CustomData.oShowWidget:Stop()
    oWidget.CustomData.oShowWidget = nil
    MrxGui.SetFadeEnabled(true)
    for uPlayer, bNeedsRestore in pairs(oWidget.CustomData.tHudStates) do
      if bNeedsRestore then
        MrxGuiManager.ToggleHud(uPlayer, true)
        oWidget.CustomData.tHudStates[uPlayer] = nil
      end
    end
  end
  if oWidget.CustomData.oSubtitle.StopSubtitles then
    oWidget.CustomData.oSubtitle:StopSubtitles()
  end
  MrxGui.SetGlobalFadeVisible(true)
end

function _HandleInitializationEvent(oWidget)
  oWidget:SetUseImmortalEvents(true)
  oWidget:SetFullscreen(true)
  local oMovieWidget = MrxGuiBase.MovieWidget:new()
  oMovieWidget:SetTransient(false)
  oWidget:AddChild(oMovieWidget)
  oWidget.CustomData.oMovieWidget = oMovieWidget
  oMovieWidget:SetAnchoring("center", "center")
  oMovieWidget:SetFullscreen("Letterbox")
  local tChildren = oWidget:GetChildren()
  tChildren[1]:SetFullscreen(true)
  oWidget.Show = Show
  oWidget._Hide = _Hide
  oWidget.Hide = HideSlow
  oWidget.HideSlow = HideSlow
  oWidget.ShowMovie = ShowMovie
  oWidget.Play = PlayMovie
  oWidget.Pause = PauseMovie
  oWidget.IsMovieRunning = IsMovieRunning
  oWidget.IsMovieHiding = IsMovieHiding
  tChildren[2]:SetFullscreen("Letterbox")
  oWidget.CustomData.oPicture = tChildren[2]
  oWidget.CustomData.nFadeOutPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 0})
  oWidget.CustomData.nFadeInPoint = oWidget:AddAnimationPoint({TranslucencyLevel = 255})
  oWidget.CustomData.oSubtitle = tChildren[5]
  oWidget.CustomData.oSubtitle.CustomData.oMovie = oMovieWidget
  oWidget.CustomData.oSuperSubtitle = tChildren[6]
  oWidget.CustomData.oPlaceholderText = tChildren[3]
  oWidget.CustomData.oBackWidget = tChildren[1]
  oWidget.CustomData.tAddOrder = {
    oWidget.CustomData.oBackWidget,
    oWidget.CustomData.oPicture,
    oWidget.CustomData.oPlaceholderText,
    tChildren[4],
    oWidget.CustomData.oMovieWidget,
    oWidget.CustomData.oSubtitle,
    oWidget.CustomData.oSuperSubtitle
  }
  oWidget.CustomData.tHudStates = {}
  for nIndex, oChild in pairs(tChildren) do
    oChild.CustomData.nFadeOutPoint = oChild:AddAnimationPoint({TranslucencyLevel = 0})
    oChild.CustomData.nFadeInPoint = oChild:AddAnimationPoint({TranslucencyLevel = 255})
    oChild:SetIgnoresPause(true)
  end
  oWidget:_Hide()
  if _GuiInternal.SetImageTextureTransience then
    _GuiInternal.SetImageTextureTransience(tChildren[2].BasicData.uId, true)
  end
end

function _HandleInputEvent(oWidget, tEvent)
  if (not Net.IsMultiplayer() or Net.IsServer()) and MrxGuiBase.Joystick.BUTTON_PAD2_D == tEvent.ButtonPress then
    oWidget:HideSlow()
  end
end

function _FadeInElements(oUnused, oWidget, nFadeInTime, tSubtitleData, nPlaceholder)
  local tChildren = oWidget:GetChildren()
  nFadeInTime = nFadeInTime or 0.2
  local oSubtitle = oWidget.CustomData.oSubtitle
  for nIndex, oChild in pairs(tChildren) do
    if oChild == oSubtitle then
      oChild:AnimateToPoint(oWidget.CustomData.nFadeInPoint, nFadeInTime, true, _ActivateCinematicState, {
        oWidget,
        tSubtitleData,
        1
      })
    elseif 1 < nIndex and nIndex < #tChildren then
      oChild:AnimateToPoint(oWidget.CustomData.nFadeInPoint, nFadeInTime, true)
    end
  end
end

function _ActivateCinematicState(oUnused, oWidget, tSubtitleData, nPlaceholder)
  MrxGuiBase.GetControlFocus(oWidget, true, true)
  Sys.RequestGameState("cinematic")
  MrxSound.EnterCinematicState()
  if oWidget.CustomData.oShowWidget then
    MrxGui.SetFadeEnabled(false)
    local tPlayers = Player.GetAllPlayers()
    for n, uPlayer in pairs(tPlayers) do
      if MrxGuiManager.GetHudState(uPlayer) then
        MrxGuiManager.ToggleHud(uPlayer, false)
        oWidget.CustomData.tHudStates[uPlayer] = true
      end
    end
    oWidget:SetVisible(false)
    oWidget.CustomData.oShowWidget:SetVisible(true)
    oWidget.CustomData.oSubtitle:SetVisible(true)
    oWidget.CustomData.oSuperSubtitle:SetVisible(true)
    oWidget:SetTranslucency(255)
    oWidget.CustomData.oShowWidget:Play()
    if "table" == type(tSubtitleData) then
      oWidget.CustomData.oSubtitle:BeginSubtitles(tSubtitleData)
    end
    MrxGui.SetGlobalFadeVisible(false)
  end
end

function _EndHideAnimation(oWidget)
  oWidget:SetVisible(false)
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    MrxGuiBase.RemoveWidget(oChild)
  end
end

function _InitializeSubtitleBuffer(oSubtitle)
  oSubtitle:Wrap()
  local nX1, nY1, nX2, nY2 = oSubtitle:GetLocation()
  oSubtitle.CustomData.nY2 = nY2
  oSubtitle.BeginSubtitles = BeginSubtitles
  oSubtitle.StopSubtitles = StopSubtitles
  local oSuperSubtitle = oSubtitle.ParentWidget:GetChildren()[6]
  oSubtitle.CustomData.oSuperSubtitle = oSuperSubtitle
end

function BeginSubtitles(oSubtitle, tRawSubtitleData)
  local tSubtitleData
  if Sys.SubtitlesEnabled and Sys.SubtitlesEnabled() then
    tSubtitleData = tRawSubtitleData
  else
    tSubtitleData = {}
    for n, tSubtitle in pairs(tRawSubtitleData) do
      if tSubtitle[4] then
        table.insert(tSubtitleData, tSubtitle)
      end
    end
  end
  if 0 < #tSubtitleData then
    table.sort(tSubtitleData, _TimeLessThan)
    local tData = oSubtitle.CustomData
    tData.tSubtitleData = tSubtitleData
    tData.nNextTime = tSubtitleData[1][1]
    tData.nNextIndex = 1
    tData.nDisplayTime = 0
    tData.sCurrentDisplay = nil
    tData.nCurrentTime = 0
    tData.sCurrentSuperDisplay = nil
    tData.sSuperDisplayTime = 0
    oSubtitle:SetEventHandler("GuiUpdate", HandleSubtitleUpdate)
  end
end

function _TimeLessThan(tData1, tData2)
  return tData1[1] < tData2[1]
end

function StopSubtitles(oSubtitle)
  local tData = oSubtitle.CustomData
  tData.tSubtitleData = nil
  tData.nNextTime = -1
  tData.nNextIndex = 1
  tData.nDisplayTime = 0
  tData.sCurrentDisplay = nil
  tData.nCurrentTime = 0
  tData.sCurrentSuperDisplay = nil
  tData.sSuperDisplayTime = 0
  oSubtitle:SetEventHandler("GuiUpdate", nil)
  oSubtitle:SetText(" ")
  oSubtitle.CustomData.oSuperSubtitle:SetText(" ")
end

function HandleSubtitleUpdate(oSubtitle, nDeltaTime)
  local tData = oSubtitle.CustomData
  local oSuperSubtitle = oSubtitle.CustomData.oSuperSubtitle
  tData.nCurrentTime = 0.033333335 * oSubtitle.CustomData.oMovie:GetCurrentFrame()
  if tData.sCurrentDisplay then
    tData.nDisplayTime = tData.nDisplayTime - nDeltaTime
    if tData.nDisplayTime <= 0 then
      tData.sCurrentDisplay = nil
      tData.nDisplayTime = 0
      oSubtitle:SetText(" ")
    end
  end
  if tData.sCurrentSuperDisplay then
    tData.sSuperDisplayTime = tData.sSuperDisplayTime - nDeltaTime
    if 0 >= tData.sSuperDisplayTime then
      tData.sCurrentSuperDisplay = nil
      tData.sSuperDisplayTime = 0
      oSuperSubtitle:SetText(" ")
    end
  end
  local sCollectedSubtitles, sCollectedSuperSubtitles
  if 0 < tData.nNextTime then
    while tData.nNextTime <= tData.nCurrentTime and 0 < tData.nNextTime do
      local tNewSubtitle = tData.tSubtitleData[tData.nNextIndex]
      if tNewSubtitle then
        if tNewSubtitle[4] then
          if not sCollectedSuperSubtitles then
            sCollectedSuperSubtitles = tNewSubtitle[2]
          else
            sCollectedSuperSubtitles = sCollectedSuperSubtitles .. "[n][n]" .. tNewSubtitle[2]
          end
          tData.sSuperDisplayTime = tNewSubtitle[3] or 3
        else
          if not sCollectedSubtitles then
            sCollectedSubtitles = tNewSubtitle[2]
          else
            sCollectedSubtitles = sCollectedSubtitles .. "[n][n]" .. tNewSubtitle[2]
          end
          tData.nDisplayTime = tNewSubtitle[3] or 3
        end
        tData.nNextIndex = tData.nNextIndex + 1
        local tNextSubtitle = tData.tSubtitleData[tData.nNextIndex]
        if tNextSubtitle then
          tData.nNextTime = tNextSubtitle[1]
        else
          tData.nNextTime = -1
        end
      else
        tData.nNextTime = -1
      end
      if sCollectedSubtitles then
        tData.sCurrentDisplay = sCollectedSubtitles
        oSubtitle:SetText(sCollectedSubtitles)
        oSubtitle:Wrap()
        oSubtitle:SetLocation(nil, oSubtitle.CustomData.nY2 - oSubtitle:GetHeight(), nil, oSubtitle.CustomData.nY2)
      end
      if sCollectedSuperSubtitles then
        tData.sCurrentSuperDisplay = sCollectedSuperSubtitles
        oSuperSubtitle:SetText(sCollectedSuperSubtitles)
        oSuperSubtitle:Wrap()
      end
    end
  end
end
