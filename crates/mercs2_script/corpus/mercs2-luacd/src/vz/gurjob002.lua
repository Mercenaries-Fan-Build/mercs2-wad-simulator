inherit("MrxTaskJobVerifySet")
_tTargetNearbyVo = {
  {
    vSequence = "Fiona-In-Mission-Job-Gur12-01"
  }
}
_tTargetCompleteVo = {
  {
    vSequence = "Fiona-In-Mission-Job-All03-08"
  },
  {
    vSequence = "Fiona-In-Mission-Job-All03-09"
  }
}

function LoadAssets(self, tSaveData)
  for i = 1, 5 do
    local n = string.format("%02d", i)
    self:_AddTarget({
      sTarget = "GurJob002_" .. n .. "_Target",
      sDefenseLayer = "vz_State_GurJob002_" .. n,
      sStagingLayer = "vz_State_GurJob002_" .. n .. "_staging",
      sPristineLayer = "vz_State_GurJob002_" .. n .. "_pristine"
    })
  end
  for i = 1, 5 do
    local n = string.format("%02d", i)
    self:_AddTarget({
      sTarget = "GurJob012_" .. n .. "_Target",
      sDefenseLayer = "vz_State_GurJob012_" .. n,
      sStagingLayer = "vz_State_GurJob012_" .. n .. "_staging",
      sPristineLayer = "vz_State_GurJob012_" .. n .. "_pristine"
    })
  end
  MrxTaskJobVerifySet.LoadAssets(self, tSaveData)
end

function Activated(self)
  MrxTaskJobVerifySet.Activated(self)
  self:_SetFactionId("Gur")
  self:_SetTargetNearbyVo(_tTargetNearbyVo)
  self:_SetTargetCompleteVo(_tTargetCompleteVo)
  self:_Go()
end
