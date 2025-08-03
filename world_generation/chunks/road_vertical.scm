(fill-area
    (fill-area
	(filled-chunk (tile 'concrete))
	(make-area
	    (make-point 2 0)
	    (make-point (- size-x 4) size-y))
	(tile 'asphalt))
    (make-area
        (make-point (- (/ size-x 2) 1) 0)
        (make-point 2 size-y))
    (tile 'asphalt-line-vertical))
