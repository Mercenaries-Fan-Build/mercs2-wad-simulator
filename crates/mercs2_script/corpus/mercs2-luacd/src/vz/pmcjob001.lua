inherit("MrxTaskJobCollectType")

function Activated(self)
  Debug.Printf("Calling PmcJob001 Activated")
  MrxTaskJobCollectType.Activated(self)
  self:_SetShortDescription("[PmcJob001.Objectives]")
  self:_SetCollectName("[PmcJob001.Title]")
  self:_SetLabelFilter("SpareParts")
  self:_SetQuota(100)
  self:_Go()
end

function Cleanup(self)
  MrxTaskJobCollectType.Cleanup(self)
end
