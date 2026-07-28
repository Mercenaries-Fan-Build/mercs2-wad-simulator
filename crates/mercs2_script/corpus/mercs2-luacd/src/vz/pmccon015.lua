inherit("MrxTaskContract")
import("MrxTaskRace")
import("MrxTaskObjectiveDeliver")
import("MrxTaskObjectiveDestroy")
import("MrxLayerManager")
import("MrxSubtitle")
import("MrxMusic")
import("MrxFactionManager")
import("MrxAchievements")

function LoadAssets(self, tSaveData)
  MrxLayerManager.Add("vz_state_PmcCon015_a", self.AssetsLoaded, {self})
end

function Activated(self)
  MrxTaskContract.Activated(self)
  local isMultiplayer = Player.GetCurrentPlayers()
  if isMultiplayer == 2 then
    mpVehicle = MrxUtil.SpawnObject("Phoenix (racing)", Pg.GetGuidByName("loc_PmcCon015_RaceCar_MP"))
    spVehicle = MrxUtil.SpawnObject("Phoenix (racing)", Pg.GetGuidByName("loc_PmcCon015_RaceCar_SP"))
    tVehicle = {spVehicle, mpVehicle}
  else
    spVehicle = MrxUtil.SpawnObject("Phoenix (racing)", Pg.GetGuidByName("loc_PmcCon015_RaceCar_SP"))
    tVehicle = {spVehicle}
  end
  local nTimeLimit = 45
  local nTimeToAdd = 10
  local nCompletions = self:GetNumCompletions()
  if nCompletions == 1 then
    nTimeLimit = 30
    nTimeToAdd = 5
  elseif 2 <= nCompletions then
    nTimeLimit = 25
    nTimeToAdd = 5
  end
  StartRace(self, tVehicle, nTimeLimit, nTimeToAdd)
  self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    tVehicle,
    "d",
    "e"
  }, MrxMusic.PlaySpecialMusic, {
    "mercs_mu_score_recruit_01"
  })
  local racerDeath = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("PmcCon015_RaceCar")
  }, self.VehicleDeath, {self})
end

function StartRace(self, tVeh, nTimeLimit, nTimeToAdd)
  self.oRaceObjective = self:CreateChild({
    sRaceMission = "PmcCon015",
    sName = "PmcCon015Race",
    sModuleName = "MrxTaskRace",
    vTgtInclude = tVeh,
    sDspShortDesc = "[PmcCon015.Objectives.003]",
    tTimerParams = {nStartTime = nTimeLimit},
    nAddTime = nTimeToAdd,
    tCourseLocs = {
      "loc_pmccon015_checkpoint_1",
      "loc_pmccon015_checkpoint_2",
      "loc_pmccon015_checkpoint_3",
      "loc_pmccon015_checkpoint_4",
      "loc_pmccon015_checkpoint_5",
      "loc_pmccon015_checkpoint_6",
      "loc_pmccon015_checkpoint_7",
      "loc_pmccon015_checkpoint_9",
      "loc_pmccon015_checkpoint_10",
      "loc_pmccon015_checkpoint_11",
      "loc_pmccon015_checkpoint_12",
      "loc_pmccon015_checkpoint_13",
      "loc_pmccon015_checkpoint_14",
      "loc_pmccon015_checkpoint_15",
      "loc_pmccon015_checkpoint_16",
      "loc_pmccon015_checkpoint_17",
      "loc_pmccon015_checkpoint_18"
    },
    fOnComplete = function()
      local uWinner = self.oRaceObjective._uWinner
      if uWinner then
        MrxAchievements.AchievementAddCount("ACHIEVEMENT_HIGHWAY_TO_HELL", 1, uWinner, true)
      end
      local isMultiplayer = Player.GetCurrentPlayers()
      if isMultiplayer == 2 then
        local uWinner = self.oRaceObjective._uWinner
        MrxAchievements.NetGrantAchievement("ACHIEVEMENT_WHEELS_OF_STEEL", uWinner)
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
end

function VehicleDeath(self)
  self:_SetCancelMessage("[PmcCon015.Terms.Cancel03]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Pmc16-04",
    3,
    {
      self.Cancel,
      {self}
    }
  })
end

function CourseUnfinished(self)
  self:_SetCancelMessage("[PmcCon015.Terms.Cancel04]")
  MrxVoSequence.Start({
    "Fiona-In-Mission-MinorContract-Pmc16-05",
    3,
    {
      self.Cancel,
      {self}
    }
  })
end

function Cleanup(self)
  Debug.Printf("*****************PmcCon015: Cleanup")
  MrxMusic.StopSpecialMusic()
  MrxLayerManager.MarkForRemoval("vz_state_PmcCon015_a")
  MrxTaskContract.Cleanup(self)
  if isMultiplayer == 2 then
    Object.Remove(mpVehicle)
    Object.Remove(spVehicle)
  else
    Object.Remove(spVehicle)
  end
end
