inherit("MrxTaskContract")
import("MrxGuiHudMessage")
import("MrxMusic")
import("MrxTimer")
import("MrxUtil")
import("MrxAchievements")
import("MrxTutorialManager")

function Activated(self)
  self.bStarted = false
  self.nCopterDead = false
  MrxTaskContract.Activated(self)
  self:_CreateEvent(Event.ObjectHibernation, {
    Player.GetLocalCharacter(),
    "awake"
  }, Start, {self})
end

function Start(self)
  local copters = {
    "PmcCon013_Copter01",
    "PmcCon013_Copter02"
  }
  for i, copter in pairs(copters) do
    local guid = Pg.GetGuidByName(copter)
    self:_CreateEvent(Event.ObjectHibernation, {guid, "awake"}, Vehicle.SetCanPlayerUse, {
      guid,
      "p",
      false
    })
  end
  PmcCon013_VehicleObjective = self:CreateChild({
    sName = "PmcCon013_VehicleObjective",
    sModuleName = "MrxTaskObjectiveEnterVehicle",
    vTgtInclude = {
      "PmcCon013_Copter01",
      "PmcCon013_Copter02"
    },
    nQuota = 1,
    tOnComplete = {
      {
        StartTheShallenge,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
end

function StartTheShallenge(self)
  if self.bStarted then
    return
  else
    self.bStarted = true
  end
  self:_CreateEvent(Event.TimerRelative, {0.5}, PmcCon013_VehicleObjective.Complete, {PmcCon013_VehicleObjective})
  MrxTutorialManager.ShowMessage("[Tutorial.Winch]", false, "PmcCon013")
  self:_CreateEvent(Event.TimerRelative, {7}, MrxTutorialManager.HideMessage, {false, "PmcCon013"})
  self:_CreatePersistentEvent(Event.ObjectDeath, {
    "PmcCon013_Copter"
  }, CopterDestroyed, {self})
  MrxMusic.PlaySpecialMusic("mu_mission_pmccon013_01")
  nTargetHeight = 5 + (self:GetNumCompletions() or 0)
  nTargetHeight = math.min(nTargetHeight, 7)
  nTicks = 0
  nPrevHeight = 0
  nPrevCount = 0
  nRadius = 10
  oMissionTimer = MrxTimer:Create({
    nStartTime = 300,
    nWarning = 60,
    iTray = 3,
    tDoneCallbacks = {
      {
        self._SetCancelMessage,
        {
          self,
          "[OilCon005.Terms.Cancel03]"
        }
      },
      {
        Cancel,
        {self}
      }
    }
  })
  oMissionTimer:Start()
  local s = string.format("[pmccon013.objective.short:%d]", nTargetHeight)
  oObjective = self:CreateChild({
    sName = "PmcCon013_Objective",
    sModuleName = "MrxTaskObjective",
    sDspShortDesc = s,
    vTgtInclude = "PmcCon013_Target",
    tOnComplete = {
      {
        self.Complete,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
  x, nStartingY, z = Object.GetPosition(Pg.GetGuidByName("PmcCon013_Target"))
  self:_CreatePersistentEvent(Event.TimerRelative, {0.25}, PollHeight, {
    Pg.GetGuidByName("PmcCon013_Target"),
    nStartingY
  })
  uMarker = Marker.AddDisc(Pg.GetGuidByName("PmcCon013_Loc"), nRadius, 255, 200, 0, 0.25)
  if Net.IsServer() then
    Net.SendEvent_AddMarkerObjective(Pg.GetGuidByName("PmcCon013_Loc"), uMarker, 255, 200, 0, 0.25, 0, nRadius, 0, true)
  end
end

function PollHeight(uGuid)
  local tX, tY, tZ = Object.GetPosition(uGuid, nStartingY)
  local nHeight = tY - nStartingY
  if nHeight > nTargetHeight and nHeight == nPrevHeight and not Object.IsWinched(uGuid) then
    nTicks = nTicks + 1
  else
    nTicks = 0
  end
  if MrxUtil.GetDistanceBetween(uGuid, Pg.GetGuidByName("PmcCon013_Loc"), true) > nRadius then
    bOOB = true
  else
    bOOB = false
    local nCount = Math.floor(nTicks / 4)
    if 3 <= nCount then
      oObjective:Complete()
    end
    nPrevHeight = nHeight
  end
  DisplayProgress(nHeight, nTargetHeight, nCount, bOOB)
end

function DisplayProgress(nHeight, nTargetHeight, nCount, bOOB)
  local sHeightString = "[white]"
  if nTargetHeight < nHeight then
    sHeightString = "[green]"
  end
  local nCurrentHeight = string.format("%.1d", nHeight)
  local s = string.format(sHeightString .. "[pmccon013.objective.target:%.1d]", nTargetHeight)
  Hud.ObjectiveTray:SetSlotToText({nSlot = 1, sText = s})
  if bOOB then
    Hud.ObjectiveTray:SetSlotToText({
      nSlot = 2,
      sText = "[red][pmccon013.objective.oob]"
    })
  else
    s = string.format(sHeightString .. "[pmccon013.objective.current:%.1d]", nCurrentHeight)
    Hud.ObjectiveTray:SetSlotToText({nSlot = 2, sText = s})
  end
end

function Complete(self)
  local numCompletions = self:GetNumCompletions()
  Debug.Printf("********************* PMCCON013: numCompletions")
  Debug.Printf(numCompletions)
  if numCompletions == 2 then
    Debug.Printf("********************* PMCCON013: IN ACHIEVEMENT BLOCK")
    MrxAchievements.NetGrantAchievement("ACHIEVEMENT_BALLS_TO_THE_WALL")
  end
  MrxTaskContract.Complete(self)
end

function Cleanup(self)
  MrxMusic.StopSpecialMusic()
  Hud.ObjectiveTray:ClearSlot({nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({nSlot = 2})
  if uMarker then
    Marker.Remove(uMarker)
  end
  if Net.IsServer() and uMarker then
    Net.SendEvent_RemoveMarkerObjective(uMarker)
  end
  if oMissionTimer then
    oMissionTimer:Stop()
  end
  MrxLayerManager.Remove({
    "vz_state_PmcCon013_MP"
  })
  MrxTaskContract.Cleanup(self)
end

function CopterDestroyed(self)
  if self.nCopterDead then
    self:_SetCancelMessage("[PmcCon016.Terms.Cancel03]")
    self:Cancel()
  else
    self.nCopterDead = true
  end
end
