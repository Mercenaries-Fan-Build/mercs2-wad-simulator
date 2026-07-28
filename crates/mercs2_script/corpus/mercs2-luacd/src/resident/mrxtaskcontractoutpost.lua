inherit("MrxTaskContract")
import("Outpost")
import("MrxFactionManager")
import("MrxAchievements")
import("MrxTutorialManager")
import("MrxStatsManager")

function LoadAssets(self, tSaveData)
  local tOutpostConfig = self:GetOutpostConfig()
  local tLayers = {}
  if tOutpostConfig.sPristineLayer then
    table.insert(tLayers, tOutpostConfig.sPristineLayer)
  end
  if tOutpostConfig.sStagingLayer then
    table.insert(tLayers, tOutpostConfig.sStagingLayer)
  end
  if tOutpostConfig.sDefenseLayer then
    table.insert(tLayers, tOutpostConfig.sDefenseLayer)
  end
  MrxLayerManager.Add(tLayers, self.AssetsLoaded, {self})
end

function Activated(self)
  MrxTaskContract.Activated(self)
  local tConfig = self:GetConfig()
  local tOutpostConfig = self:GetOutpostConfig()
  _oOutpost = Outpost:Create({
    sOutpost = tOutpostConfig.sOutpostBldg,
    sBoundary = tOutpostConfig.sCapturePt,
    tCapturePts = tOutpostConfig.tCapturePts,
    sDefenders = MrxFactionManager.GetFactionTemplateName(tOutpostConfig.sRivalFaction),
    sAttackers = MrxFactionManager.GetFactionTemplateName(tConfig.sFactionId),
    tDBSpawners = tOutpostConfig.tDangerousBldgs,
    nStartingHealth = tOutpostConfig.nStartingHealth,
    nRusherQuota = tOutpostConfig.nRusherQuota,
    fUpdatedCallback = function()
      if self.bCompletedFirstTutorial == true then
        if self._UpdatedTimer then
          Event.Delete(self._UpdatedTimer)
        end
        self._UpdatedTimer = Event.Create(Event.TimerRelative, {6}, self.NoProgressMade, {self})
        if self._ShowOutpostTutorial then
          MrxTutorialManager.HideMessage(false, "OutpostCapture")
          Event.Delete(self._ShowOutpostTutorial)
          self._ShowOutpostTutorial = Event.Create(Event.TimerRelative, {15}, self.ShowOutpostTutorial, {self})
        end
      end
    end
  })
  self:CreateChild({
    sName = "Outpost",
    sModuleName = "MrxTaskObjectiveCaptureOutpost",
    uOutpostBldg = Pg.GetGuidByName(tOutpostConfig.sOutpostBldg),
    vTgtInclude = tOutpostConfig.tCapturePts,
    sDspShortDesc = tOutpostConfig.sDspShortDesc or "[Generic.ObjectiveOutpost]",
    fOnComplete = function()
      MrxStatsManager.IncreaseOutpostCapturedCounter()
      self:Complete()
    end,
    fOnCancel = function()
      if _oOutpost.bDestroyed then
        self:_SetCancelMessage("[Fanfare.Cancel.OutpostDestroyed]")
      end
      self:Cancel()
    end
  })
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName(tOutpostConfig.sOutpostBldg),
    "<",
    100,
    false,
    false
  }, Near, {self})
end

function NoProgressMade(self)
  if not self._ShowOutpostTutorial then
    self:ShowOutpostTutorial()
    self.nTutorialText = 1
  end
end

function Near(self)
  RemoveTutorialEvents(self)
  local tConfig = self:GetConfig()
  local tOutpostConfig = self:GetOutpostConfig()
  self._Far = Event.Create(Event.ObjectProximity, {
    Player.GetLocalCharacter(),
    Pg.GetGuidByName(tOutpostConfig.sOutpostBldg),
    ">",
    100,
    false,
    false
  }, Far, {self})
  self:SetupTutorialTimers()
end

function SetupTutorialTimers(self)
  self.nTutorialText = 1
  self:ShowOutpostTutorial()
end

function Far(self)
  MrxTutorialManager.HideMessage(false, "OutpostCapture")
  RemoveTutorialEvents(self)
  local tConfig = self:GetConfig()
  local tOutpostConfig = self:GetOutpostConfig()
  self._Near = Event.Create(Event.ObjectProximity, {
    Player.GetLocalCharacter(),
    Pg.GetGuidByName(tOutpostConfig.sOutpostBldg),
    "<",
    100,
    false,
    false
  }, Near, {self})
end

function ShowOutpostTutorial(self)
  local tConfig = self:GetConfig()
  local sFaction = MrxFactionManager.GetShortPlayerVisibleName(tConfig.sFactionId)
  local sFactionAdjective = MrxFactionManager.GetAdjective(tConfig.sFactionId)
  local sFactionTemplate = MrxFactionManager.GetFactionTemplateName(tConfig.sFactionId)
  local sFactionSupport = Outpost.GetFactionSupportName(sFactionTemplate)
  local nNextWaitTime = 6
  if self.nTutorialText == 1 then
    MrxTutorialManager.BeginCustomTutorial("OutpostCapture")
    MrxTutorialManager.ShowMessage("[Tutorial.OutpostCapture.Key1:" .. sFaction .. "]", false, "OutpostCapture")
    self.nTutorialText = self.nTutorialText + 1
  elseif self.nTutorialText == 2 then
    MrxTutorialManager.ShowMessage("[Tutorial.OutpostCapture.Key2:" .. sFactionAdjective .. "]", false, "OutpostCapture")
    self.nTutorialText = self.nTutorialText + 1
  elseif self.nTutorialText == 3 then
    MrxTutorialManager.ShowMessage("[Tutorial.OutpostCapture.Key3:" .. sFactionSupport .. ":" .. sFactionAdjective .. "]", false, "OutpostCapture")
    self.nTutorialText = self.nTutorialText + 1
  elseif self.nTutorialText == 4 then
    MrxTutorialManager.ShowMessage("[Tutorial.OutpostCapture.Key4]", false, "OutpostCapture")
    self.nTutorialText = self.nTutorialText + 1
  else
    MrxTutorialManager.EndCustomTutorial("OutpostCapture")
    nNextWaitTime = 29
    self.bCompletedFirstTutorial = true
    self.nTutorialText = 1
  end
  self._ShowOutpostTutorial = Event.Create(Event.TimerRelative, {nNextWaitTime}, ShowOutpostTutorial, {self})
end

function Complete(self)
  if _oOutpost and not _oOutpost.bCaptured and not _oOutpost.bDestroyed then
    _oOutpost:Captured()
  end
  local tOutpostConfig = self:GetOutpostConfig()
  if tOutpostConfig.sStagingLayer then
    MrxLayerManager.MarkForRemoval(tOutpostConfig.sStagingLayer)
  end
  if tOutpostConfig.sDefenseLayer then
    MrxLayerManager.MarkForRemoval(tOutpostConfig.sDefenseLayer)
  end
  if tOutpostConfig.sCapturedLayer then
    MrxLayerManager.MarkForAddition(tOutpostConfig.sCapturedLayer)
  end
  MrxTaskContract.Complete(self)
end

function RemoveTutorialEvents(self)
  if self._ShowOutpostTutorial then
    Event.Delete(self._ShowOutpostTutorial)
    self._ShowOutpostTutorial = nil
  end
  if self._TutorialTimer then
    Event.Delete(self._TutorialTimer)
    self._TutorialTimer = nil
  end
  if self._Near then
    Event.Delete(self._Near)
    self._Near = nil
  end
  if self._Far then
    Event.Delete(self._Far)
    self._Far = nil
  end
  if self._UpdatedTimer then
    Event.Delete(self._UpdatedTimer)
    self._UpdatedTimer = nil
  end
  Debug.Printf("RemoveTutorialEvents hidemessage")
  MrxTutorialManager.HideMessage(false, "OutpostCapture")
end

function Cleanup(self)
  if _oOutpost and not _oOutpost.bCaptured and not _oOutpost.bDestroyed then
    _oOutpost:Delete()
  end
  RemoveTutorialEvents(self)
  MrxTaskContract.Cleanup(self)
end

function GetOutpostConfig(self)
  local tConfig = self:GetConfig()
  return tConfig.tOutpostConfig
end
