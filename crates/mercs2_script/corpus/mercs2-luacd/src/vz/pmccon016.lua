inherit("MrxTaskContract")
import("MrxTaskRace")
import("MrxTaskObjectiveDeliver")
import("MrxSubtitle")
import("MrxLayerManager")
import("MrxMusic")
import("MrxAchievements")
import("MrxTutorialManager")

function LoadAssets(self, tSaveData)
  MrxLayerManager.Add("vz_state_PmcCon016_a", self.AssetsLoaded, {self})
end

function Activated(self)
  MrxTaskContract.Activated(self)
  self:GetVehicles()
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Pmc16-01"
  })
  nTimeLimit = 75
  nTimeToAdd = 15
  local nCompletions = self:GetNumCompletions()
  Debug.Printf(nCompletions)
  if nCompletions == 1 then
    nTimeLimit = 60
    nTimeToAdd = 10
  elseif 2 <= nCompletions then
    nTimeLimit = 55
    nTimeToAdd = 10
  end
  self:_CreateEvent(Event.ObjectHibernation, {tVehicle, "awake"}, Vehicle.Usable, {uVehicle, false})
  StartRace(self, tVehicle, nTimeLimit, nTimeToAdd)
  local racerDeath = self:_CreateEvent(Event.ObjectDeath, {tVehicle}, self.VehicleDeath, {self})
  MrxMusic.PlaySpecialMusic("mu_PmcCon016_01")
  self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAnyCharacter(),
    Pg.GetGuidByName("loc_pmccon016_005"),
    "<",
    5,
    false,
    false
  }, FionaProximityVO, {self})
  RaceTutoral(self)
end

function RaceTutoral(self)
  MrxTutorialManager.ShowMessage("[PmcCon016.Terms.Tutorial]")
end

function GetVehicles(self)
  local isMultiplayer = Player.GetCurrentPlayers()
  if isMultiplayer == 1 then
    tVehicle = MrxUtil.SpawnObject("Panhard (Assault)", Pg.GetGuidByName("loc_PmcCon016_Racer"))
  elseif isMultiplayer == 2 then
    tVehicle = MrxUtil.SpawnObject("Buggy (Hellfire)", Pg.GetGuidByName("loc_PmcCon016_Racer"))
  end
end

function StartRace(self, tVehicle, nTimeLimit, nTimeToAdd)
  self.oRaceObjective = self:CreateChild({
    sRaceMission = "PmcCon016",
    sName = "PmcCon016Race",
    sModuleName = "MrxTaskRace",
    sDspShortDesc = "[PmcCon016.Objectives.002]",
    vTgtInclude = tVehicle,
    tTimerParams = {nStartTime = nTimeLimit},
    tCourseLocs = {
      "loc_pmccon016_001",
      "loc_pmccon016_002",
      "loc_pmccon016_003",
      "loc_pmccon016_004",
      "loc_pmccon016_005",
      "loc_pmccon016_006",
      "loc_pmccon016_1",
      "loc_pmccon016_2",
      "loc_pmccon016_3",
      "loc_pmccon016_4",
      "loc_pmccon016_5",
      "loc_pmccon016_6",
      "loc_pmccon016_7",
      "loc_pmccon016_8",
      "loc_pmccon016_9",
      "loc_pmccon016_10",
      "loc_pmccon016_11",
      "loc_pmccon016_12",
      "loc_pmccon016_13",
      "loc_pmccon016_14",
      "loc_pmccon016_15",
      "loc_pmccon016_16",
      "loc_pmccon016_17",
      "loc_pmccon016_18",
      "loc_pmccon016_19",
      "loc_pmccon016_20",
      "loc_pmccon016_21"
    },
    fOnComplete = function()
      local uWinner = self.oRaceObjective._uWinner
      if uWinner then
        MrxAchievements.AchievementAddCount("ACHIEVEMENT_HIGHWAY_TO_HELL", 1, uWinner, true)
      end
      MrxVoSequence.Start({
        "Fiona-In-Mission-MinorContract-Pmc16-03",
        3,
        {
          self.Complete,
          {self}
        }
      })
    end,
    fOnCancel = function()
      self:CourseUnfinished()
    end
  })
  self:CreateChild({
    sName = "DestructionTargets",
    sModuleName = "MrxTaskObjectiveDestroy",
    sDspShortDesc = "[PmcCon016.Objectives.005]",
    bIsDestruction = true,
    sDspBlpWldIcon = "HUD_objective_timer",
    bDspBlpRdr = false,
    bDspBlpPda = false,
    nAddTime = nTimeToAdd,
    bTrackOnActivate = false,
    vTgtInclude = {
      "PmcCon016_Target1",
      "PmcCon016_Target2",
      "PmcCon016_Target3",
      "PmcCon016_Target4",
      "PmcCon016_Target5",
      "PmcCon016_Target6"
    },
    fOnPartComplete = function()
      self:ObjectDestroyed()
    end,
    fOnCancel = function()
      self:CourseUnfinished()
    end
  })
end

function ObjectDestroyed(self)
  MrxTutorialManager.HideMessage(self)
  if self.oRaceObjective then
    Debug.Printf("**********PMCCON016: PYLON DESTROYED")
    self.oRaceObjective._oTimer:AddTime(nTimeToAdd)
    Hud.ObjectiveTray:SetSlotToText({
      nSlot = 2,
      sText = "[green]" .. tostring(nTimeToAdd) .. " " .. "[PmcCon016.Terms.Bonus]"
    })
  end
  self:_CreateEvent(Event.TimerRelative, {2}, function()
    Hud.ObjectiveTray:SetSlotToText({nSlot = 2, sText = " "})
  end)
end

function FionaProximityVO(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Pmc16-02"
  })
end

function VehicleDeath(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Pmc16-04",
    3,
    {
      self.Cancel,
      {self}
    }
  })
  self:_SetCancelMessage("[PmcCon016.Terms.Cancel03]")
end

function VehicleUnentered(self)
  self:_SetCancelMessage("[PmcCon016.Terms.Cancel01]")
  self:Cancel()
end

function CourseUnfinished(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Pmc16-05",
    3,
    {
      self.Cancel,
      {self}
    }
  })
  self:_SetCancelMessage("[PmcCon016.Terms.Cancel02]")
end

function Cleanup(self)
  MrxMusic.StopSpecialMusic()
  MrxLayerManager.MarkForRemoval("vz_state_PmcCon016_a")
  MrxTaskContract.Cleanup(self)
  Object.Remove(tVehicle)
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 1})
  Hud.ObjectiveTray:ClearSlot({vPlayer = nil, nSlot = 2})
end
